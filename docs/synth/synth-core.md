# synth-core: the shared synth layer

`crates/synth-core` is the part of the audio stack that both engines use. It is `no_std`, f32, has no heap, and
depends only on `libm`. It has two halves:

1. **Building blocks** for engines: the `Device` trait and its event slots, the `VoicePool`, DSP primitives, maths, the
   bump `Pool`, the `Rng` trait and the denormal guard.
2. **`mix`** (spec `synth-mix`): the song blob decoder (`Song`), the device registry trait (`Kinds`) and the song
   `Player`.

Source: `crates/synth-core/src/`. Per-file record: `crates/synth-core/AGENTS.md`. Engines: [`wavesabre.md`](wavesabre.md),
[`tunefish4.md`](tunefish4.md).

## Module map

```mermaid
flowchart LR
    subgraph mix["mix/"]
        song["song.rs<br/>Song::parse, views"]
        reg["registry.rs<br/>Kinds, Env, Either&lt;A,B&gt;"]
        player["player.rs<br/>Player, DEFAULT_SUB = 100"]
    end
    device["device.rs<br/>Device, Io, Ctx, Events&lt;N&gt;"]
    voice["voice.rs<br/>VoicePool, Voice, Steal, Mode"]
    subgraph dsp["dsp/"]
        comb["Comb"]
        ap["AllPass"]
        ring["Ring"]
        bq["Biquad + Coefs"]
        op["OnePole"]
    end
    pool["pool.rs<br/>Pool (bump)"]
    rng["rng.rs<br/>Rng trait"]
    den["denormal.rs<br/>Guard (FTZ/DAZ)"]
    math["math.rs"]

    player --> song
    player --> reg
    player --> device
    player --> den
    reg --> pool
    reg --> device
    voice --> pool
    device --> rng
```

## The device contract

Every instrument and every effect implements one trait (`device.rs`):

| Method | Contract |
|---|---|
| `param_count()` | Number of normalized parameters, constant for the device's life |
| `set_param(i, v)` | `v` normally in 0..1, not clamped; `i ≥ param_count` is ignored |
| `note_on(note, vel, offset)` / `note_off(note, offset)` | Queue an event `offset` frames after the start of the next `run`. Velocity 0 stays a note-on here; formats where 0 means off (`.tfm`) convert in their own decoder |
| `all_notes_off()` | Release all voices, drop queued events |
| `event_lookahead()` | `L` frames: the mixer delivers events with frame `< t + n + L`. A device with internal `B`-frame blocks needs `L = B − 1` |
| `reset()` | Back to the freshly loaded state (params kept). Required: seek uses it |
| `run(io, n, ctx)` | Render `n` frames in place |

### Channels

```mermaid
flowchart LR
    subgraph Io["Io { ch: [4] }"]
        ML["ch[0] MAIN_L"]
        MR["ch[1] MAIN_R"]
        SL["ch[2] SIDE_L"]
        SR["ch[3] SIDE_R"]
    end
    inst["Instrument"] -->|overwrites| ML & MR
    fx["Effect"] -->|"reads + writes in place"| ML & MR
    SL & SR -.->|"key input (read only),<br/>e.g. Smasher sidechain"| fx
```

- Instruments **overwrite** main L/R. Whatever a receive put into main before the instrument is replaced.
- Effects transform main L/R and may read the sidechain pair.
- No device writes the sidechain pair; only track receives fill it.

### Context

`Ctx` replaces upstream globals: `rate` (Hz), `tempo` (BPM), `frame` (absolute song frame of this call's first frame,
used by block-based devices to stay on an absolute grid) and `rng` (the song's random source, `&mut dyn Rng`).

### Event slots

`Events<N>` is upstream `SynthDevice`'s fixed event array: a new event takes the first free slot, a full array drops
it, and due events fire in slot order. `Events::run(n, f)` splits one call at event offsets:

```mermaid
sequenceDiagram
    participant Dev as device.run(n)
    participant Ev as Events::run
    participant V as voices
    Dev->>Ev: run(n, step)
    loop until start == n
        Ev->>V: Step::Event{at: start} for every slot with offset 0 (slot order)
        Ev->>Ev: len = min(n - start, smallest pending offset)
        Ev->>V: Step::Render{start, len}
        Ev->>Ev: every pending offset -= len
    end
    Note over Ev: events with offset ≥ n stay queued, reduced by n
```

So a WaveSabre instrument renders sample-accurately: a note at offset 37 starts at frame 37 of the call.

## VoicePool

`VoicePool<'a, V>` owns `n` voices from the pool and implements upstream `SynthDevice` allocation. Voice types
implement `VoiceState` (`is_on`, `note`, `kill`) and `Voice<C>` (`note_on`, `note_off`, `slide`) where `C` is the engine's
voice context (`WsCx` for WaveSabre, `NoteCx` for Tunefish).

| Setting | Meaning |
|---|---|
| `limit` | Only slots `0..limit` are used (Tunefish `GenPoly`) |
| `unison` U | One note starts U voices |
| `detune`, `pan_spread` | Per unison voice: `f = j/(U−1)` counting down from 1, detune `f·detune`, pan `(f − .5)(2·pan_spread − 1) + .5` |
| `steal` | `Steal::None` (WaveSabre: no free slot → voice dropped) or `Steal::Oldest` (Tunefish: max age, lowest index on ties; `tick()` ages per block) |
| `mode` | `Mode::Poly` or `Mode::MonoLegato` (note log + held flags) |

```mermaid
flowchart TD
    on["note_on(note, vel)"] --> mono{"mode = MonoLegato?"}
    mono -- no --> start
    mono -- yes --> log["held[note] = true, push to log"]
    log --> act{"mono voice already active?"}
    act -- yes --> slide["slide every on-voice to note"]
    act -- no --> start
    start["start: repeat U times"] --> free{"free slot in k..limit?"}
    free -- yes --> use["use it"]
    free -- no --> st{"steal = Oldest?"}
    st -- yes --> old["take the oldest slot"]
    st -- no --> drop["stop (remaining unison voices dropped)"]
    use --> init["voice.note_on(note, vel, f·detune, pan)<br/>age = 0"]
    old --> init

    off["note_off(note)"] --> m2{"mode"}
    m2 -- Poly --> rel["release voices playing note"]
    m2 -- MonoLegato --> back["held[note] = false; if it was the latest,<br/>slide back to the latest still-held note,<br/>release all if none"]
```

`set_mode` kills all voices. Upstream's log overflow and empty-log read are guarded.

## DSP primitives (`dsp/`)

A primitive lives here only when at least two engines use the same algorithm.

| Type | Algorithm | Used by |
|---|---|---|
| `Comb<'a>` | Freeverb comb over a caller buffer: `out = buf; z = out(1−damp) + z·damp; buf = in + z·fb` | Cathedral, TF reverb |
| `AllPass<'a>` | Freeverb allpass: `out = buf − in; buf = in + buf·fb` | Cathedral, TF reverb |
| `Ring<'a>` | Circular delay: `write`, `oldest()` (read-then-write = len-sample delay), `at(k)`, `read_frac(d)`. `set_len` clears on change (WaveSabre `DelayBuffer`), `set_len_keep` keeps contents (Tunefish) | Echo, Chamber, Smasher, Twister, Cathedral pre-delay; TF delay, chorus, flanger |
| `Biquad` + `Coefs` | DF1, evaluated as `c1·x + c2·x1 + c3·x2 − c4·y1 − c5·y2` (WaveSabre op order). RBJ `lowpass/highpass/peak(cos, α)` take the engine's own sin/cos for parity | `WsBiquad` (Leveller), TF RBJ HP/BP, notch |
| `OnePole` | `z += k(x − z)` | TF EQ |

Other helpers: `math.rs` (`note_to_hz`, `db_to_gain` = 10^(dB/20), equal-power `pan_left/right` = √(1−p), √p, `mix`,
`clamp`, `exp2`), `denormal::Guard` (sets x86 MXCSR FTZ+DAZ or AArch64 FPCR.FZ for a render call, restores on drop),
`Pool` (bump allocator: zeroed aligned slices, `uninit` for values that themselves allocate from the pool, `need::<T>(n)`
upper bound; a failed request consumes nothing; destructors never run), `Rng` (one method, `next_f32`; the engine
chooses law and seed).

## The song blob (`mix/song.rs`)

The `synth-mix` format is WaveSabre's song blob, as written by `WaveSabreConvert`. Little endian:

```text
tempo i32, rate i32, length f64 (seconds)
devices: n i32, n × { kind u8, size i32, chunk[size] }
lanes:   n i32, n × { events i32, events × { delta i32, note u8 (bit 7 = note-off), [velocity u8 if note-on] } }
tracks:  n i32, n × { volume f32,
                      receives i32 × { track i32, channel i32 (0 main | 2 sidechain), volume f32 },
                      devices  i32 × { device index i32 },
                      lane i32,
                      automations i32 × { device (index into this track's devices) i32, param i32,
                                          points i32 × { delta i32 (signed), value u8 } } }
```

```mermaid
erDiagram
    SONG ||--o{ DEVICE : "devices[]"
    SONG ||--o{ LANE : "lanes[]"
    SONG ||--o{ TRACK : "tracks[] (last = master)"
    TRACK ||--o{ RECEIVE : receives
    TRACK }o--o{ DEVICE : "device chain (indices)"
    TRACK }o--|| LANE : "lane index"
    TRACK ||--o{ AUTOMATION : automations
    RECEIVE }o--|| TRACK : "from track"
    AUTOMATION ||--o{ POINT : points
    LANE ||--o{ EVENT : events
    DEVICE {
        u8 kind
        bytes chunk
    }
    EVENT {
        i32 delta
        u8 note
        u8 velocity
    }
    RECEIVE {
        i32 track
        i32 channel
        f32 volume
    }
    AUTOMATION {
        i32 device
        i32 param
    }
    POINT {
        i32 delta
        u8 value
    }
```

`Song::parse` validates once: counts against remaining bytes (a count is rejected before any loop if it cannot fit),
every index, receive channels 0/2, trailing bytes, tempo and rate > 0. After that the views (`devices()`, `lanes()`,
`tracks()`, `automations()`) read the borrowed bytes without failing. Note deltas are ≥ 0; automation times are signed.
Automation values are `byte/255`.

Device chunks follow upstream `Device::SetChunk`: `(size − 4)/4` little-endian f32 parameters, then the i32 size. A
short chunk leaves the remaining parameters at their defaults. Sample devices put their sample data first.

## Registry (`mix/registry.rs`)

`synth-core` cannot depend on the engines, so each engine provides an enum of its device kinds that implements
`Kinds<'a>: Device`:

- `need(kind, chunk, env)` → pool bytes for that device;
- `load(kind, chunk, env, pool, rng)` → the device; load-time random draws come from the song rng.

`Env { rate, tempo, bank: Option<&[u8]> }` carries the GM.DLS bytes for Adultery. `Either<A, B>` combines two
registries for a mixed-engine song: it tries `A`, and falls back to `B` on `UnsupportedKind`. Example:
`Either<WsDevice, TfDevice>` plays WaveSabre kinds 0–12 and Tunefish kinds 32–39 in one song.

## Player (`mix/player.rs`)

`Player<'a, K: Kinds, R: Rng + Clone>` is a single-threaded port of upstream `SongRenderer`.

### Load

```mermaid
flowchart TD
    A["Player::required_pool(song, env, sub)"] --> B["caller allocates one byte slice → Pool"]
    B --> C["load_devices: every device in song order<br/>(kinds checked first; Kinds::load draws from rng)"]
    C --> D["check every automation param < param_count"]
    D --> E["track order: repeated passes in index order,<br/>take each track whose senders are done,<br/>until the master (last track) is done"]
    E -->|"a pass adds nothing"| X["BadIndex (receive cycle)"]
    E --> F["per track: 4 × sub-frame buffers,<br/>automation states, lookahead = max device L"]
    F --> G["rng0 = snapshot of rng after load"]
```

### Render: one sub-block (`sub` = 100 frames by default)

```mermaid
flowchart TD
    S["render_sub(t0)"] --> T{"next track in dependency order"}
    T --> E1["1. events: deliver lane events with frame < t0 + sub + lookahead<br/>to every device on the track (note_on/off with offset)"]
    E1 --> E2["2. automations: value at t0 (linear between points,<br/>held after the last) → device.set_param"]
    E2 --> E3["3. clear the 4 track buffers"]
    E3 --> E4["4. receives: sender main L/R × volume<br/>→ channel 0 (main) or 2 (sidechain)"]
    E4 --> E5["5. devices in chain order: run(io, sub, ctx)"]
    E5 --> E6["6. × track volume"]
    E6 --> T
    T -->|"all tracks done"| M["master buffers feed render_f32 / render_i16"]
```

A track's signal path:

```mermaid
flowchart LR
    t1["track A main"] -->|"× recv volume"| main
    t2["track B main"] -->|"× recv volume"| side
    lane["lane events"] --> d1
    auto["automation"] -.-> d1 & d2
    subgraph track["track (4 buffers)"]
        main["main L/R"] --> d1["device 1<br/>(instrument: overwrites main)"]
        d1 --> d2["device 2<br/>(effect)"]
        side["sidechain L/R"] -.-> d2
        d2 --> vol["× track volume"]
    end
    vol --> next["receivers / master"]
```

- `render_f32(out)` and `render_i16(out)` fill interleaved stereo of any length from the master's sub-block buffer. i16
  uses upstream's `clamp(trunc(x·32767))`. The denormal guard is on for the call.
- `seek(frame)` resets every device and track, restores `rng0`, then re-renders silently up to `frame`.
- `length_frames()` = `trunc(length · rate)`.

### Determinism

All voices of all devices draw from one song random source, in track render order. Output does not depend on the
caller's buffer size, but it does depend on `sub`: the span length decides which voice gets which random number when
voices overlap. Upstream renders in 100-frame steps, so references are rendered with `DEFAULT_SUB = 100` (Global
Groove is bit-identical at 100 frames and looks like 0.9 dB SNR at 4096).

### Memory

```mermaid
flowchart LR
    subgraph pool["caller byte slice (Pool)"]
        d["devices<br/>(voices, delay lines,<br/>decoded GSM samples,<br/>TF effect pools)"] --> ts["TrackState × tracks"] --> ord["order + done flags"] --> buf["per track: 4 × sub f32<br/>+ automation states"]
    end
```

`required_pool` is an upper bound that includes alignment padding; a pool one byte short of the measured use fails with
`OutOfMemory`. Adultery reads GM.DLS waves in place from `Env::bank`, so the bank costs no pool space.
