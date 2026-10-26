# Tunefish 4 engine (`crates/tfsynth`)

An MIT `no_std` f32 synth that sounds like Brain Control's Tunefish 4. It is our own implementation, written from the
behavioural spec `research/tunefish4-functional-spec.md`; the upstream GPL source was read for behaviour only (no
copied code, comments, tables or identifiers). It runs on [`synth-core`](synth-core.md) (voice pool, Freeverb
comb/allpass, `Ring`, `Biquad`, `OnePole`, `Pool`, `Device`).

Fixed format: 44 100 Hz, stereo, 512-frame blocks. All control (envelopes, LFOs, mod matrix, filter coefficients,
wavetable rebuilds) runs once per block; audio runs per sample.

Per-file record: `crates/tfsynth/AGENTS.md`. Specs: `openspec/specs/tf-synth/spec.md`, `openspec/specs/tf-song/spec.md`.

## Crate structure

```mermaid
flowchart TB
    subgraph entry["entry points"]
        device["device.rs<br/>TfDevice (Kinds): TfInstrument kind 32,<br/>TfEffect kinds 33–39"]
        player["player.rs<br/>TfmPlayer"]
        tfm["tfm.rs<br/>Tfm::parse (.tfm)"]
    end
    instrument["instrument.rs<br/>Instrument: 16 voices, bend, wheel,<br/>fx chain, auto-sleep"]
    voice["voice.rs<br/>Voice: pitch, generator, filters"]
    modmx["modmx.rs<br/>ModMatrix (8 live slots)"]
    env["env.rs<br/>Adsr (block rate)"]
    lfo["lfo.rs<br/>Lfo (block rate)"]
    wt["wavetable.rs<br/>Table: spectrum → phase stage → IFFT"]
    ifft["ifft.rs<br/>ifft512"]
    noise["noise.rs<br/>Noise"]
    filter["filter.rs<br/>Ladder, Rbj (HP/BP), Notch"]
    subgraph fx["fx/"]
        chain["mod.rs<br/>Chain (10 slots), Fx enum"]
        k["distortion, delay, chorus,<br/>flanger, reverb, formant, eq"]
    end
    params["params.rs<br/>112-value patch, mappings"]
    tables["tables.rs<br/>once-init static tables"]
    rng["rng.rs<br/>Xorshift"]
    core["synth-core"]

    device --> instrument
    device --> chain
    player --> tfm
    player --> instrument
    instrument --> voice
    instrument --> chain
    chain --> k
    voice --> modmx --> env & lfo
    voice --> wt --> ifft
    voice --> noise --> filter
    voice --> filter
    voice & wt & noise & lfo --> tables
    instrument --> rng
    instrument & filter & k --> core
```

## Instrument

```mermaid
flowchart LR
    ev["note_on / note_off<br/>pitch_bend / mod_wheel"] --> pool["VoicePool × 16<br/>Steal::Oldest, limit = GenPoly"]
    pool --> v1["voice"] & v2["voice"] & vn["…"]
    v1 & v2 & vn -->|"× gain_curve(Gain)"| bus(("Σ"))
    bus --> chain["effect chain<br/>10 serial slots"]
    chain --> out["out L/R<br/>(512 frames)"]
    bus -.-> sleep["silence timer"]
    sleep -.->|"2 s below 1e-5 → skip effects"| chain
```

`Instrument::render(out)`:

1. Turn on flush-to-zero for the call (`denormal::Guard`), as the reference does. Without it the decaying ladder state
   goes subnormal and rendering is roughly 2× slower.
2. Clear `out`, render every active voice (held or still sounding) into it, and age the voices (`tick`).
3. Run the effect chain unless asleep.
4. Advance the auto-sleep timer: if the mean |x| of the block is below 1e-5, add 512/44100 s. Any voice rendering
   resets it. At 2 s the effects stop processing until a voice renders again.

Voice handling:

- **Polyphony.** 16 voice slots; `GenPoly` limits use to `trunc(15p + 1)` slots. When none is free, the oldest voice is
  stolen and hard-restarted.
- **Velocity 0** is a note-off.
- **Pitch bend** ∈ [−1, 1] applies to all voices, held or not. **Mod wheel** ∈ [0, 1] is mod source 5 from the next
  block. Songs never carry either (live input only).
- **LFO phase.** An LFO with Sync < 0.5 starts a new note at the instrument's free-running phase (the phase of the last
  voice rendered); with Sync ≥ 0.5 it starts at 0.
- **Randomness** comes from the instrument's own fixed-seed xorshift (the reference seeds from entropy).

## Voice: one 512-frame block

```mermaid
flowchart TD
    t["time += 1"] --> mm["mod matrix step<br/>(ADSR 1/2, LFO 1/2, wheel → M(dst))"]
    mm --> gate{"no envelope source running<br/>and key up?"}
    gate -- yes --> v0["velocity = 0 (organ gate closes)"]
    gate -- no --> vel["velocity = vel/128"]
    v0 & vel --> nz["noise → overwrites tmp L/R"]
    nz --> pitch["pitch: note table → bend → slop → glide"]
    pitch --> tb{"time % 4 == 1?"}
    tb -- yes --> rebuild["wavetable update (if shape changed)<br/>+ phase stage + IFFT"]
    tb -- no --> gen
    rebuild --> gen["generator: unison layers<br/>add into tmp L/R"]
    gen --> lp{"LP on?"} -- yes --> LP["ladder LP"]
    lp -- no --> hp
    LP --> hp{"HP on?"} -- yes --> HP["RBJ HP"]
    hp -- no --> bp
    HP --> bp{"BP on?"} -- yes --> BP["RBJ BP"]
    bp -- no --> nt
    BP --> nt{"notch on?"} -- yes --> NT["notch"]
    nt -- no --> mix
    NT --> mix["out += tmp × gain_curve(Gain)<br/>sounding = Σ|tmp| > 1"]
```

Signal path in one picture:

```mermaid
flowchart LR
    noise["noise<br/>white table × NoiseOn·vel·M"] --> sum(("+"))
    gen["wavetable generator<br/>1–10 unison layers"] --> sum
    sum --> lp["LP ladder"] --> hp["HP RBJ"] --> bp["BP RBJ"] --> nt["notch"] --> g["× gain curve"] --> bus["instrument bus"]
    mm["mod matrix M(dst)"] -.-> noise & gen & lp & hp & bp & nt
```

Amplitude shaping comes only from the mod matrix. The generator level is `GenVol·4·velocity·M(volume)`; an envelope
must be routed to `volume` (target 5) to get an ADSR shape. With no envelope routed, the note is organ-gated: full
level while the key is held, silent after release.

### Pitch

```text
base  = note_hz[note]                         # 128-entry f64-built table, semitone steps from 6.875·r³ Hz
up    = base × 1.0594631^bend_up_semitones     # trunc(12p + 1), repeated products
down  = base / 1.0594631^bend_down_semitones
f     = lerp(down, base, clamp(bend + 1)) then lerp(f, up, clamp(bend))   # linear in Hz
f    += slop · GenSlop³ · 8                    # slop ∈ [−1, 1), drawn at note-on
freq += (f − freq) / (10·GenGlide + 1)         # per block, when glide > 0 (state never reset)
```

### Wavetable generator (`wavetable.rs`)

Every fourth block (time % 4 == 1) the voice rebuilds its 512-sample single-cycle table, but only when the inputs
changed (counts exactly, floats by more than 1e-5).

```mermaid
flowchart LR
    p["GenHarmn, GenBandw, GenDamp,<br/>GenScale (× M, clamped)<br/>+ band limit K(freq)"] --> spec["magnitude spectrum<br/>512 bins, re = im = A_k"]
    spec --> phase["phase stage<br/>(GenMod, drift μ)"]
    phase --> ifft["ifft512<br/>radix-2, no scaling"]
    ifft --> norm["peak-normalise (floor 1e-5)<br/>remove mean"]
    norm --> wave["wave[512]"]
```

Spectrum, for partial `h = 1..H`:

| Quantity | Formula |
|---|---|
| Partial count H | `1 + min(trunc(64·harmonics), 64)` (1..65) |
| Centre bin | `1 + scale·(2h − 1)`, scale ∈ [0, 4] |
| Width | `0.3 + bandwidth·h` bins |
| Gain | `h^−(1 + damp)` |
| Shape | `A_k += e^(−d)·gain` (from the `exp` table) for `d = \|centre − k\|/width < 5` |
| Band limit K | `max(4, trunc(512·(1 − r)³))`, `r = clamp((freq − 8)/2000, 0, 1)`; bins ≥ K are zero |
| DC bin | `1 + 0i` |

K uses the glided keyboard frequency before octave and fixed-frequency mode, so those can alias (as in the reference).

**Phase stage.** With drift μ ≈ 0 the spectrum passes unchanged. Otherwise bin `i` (strength `i/512`) gets a table
offset `s = trunc(μ·512·(1 − GenMod·ρ_i)·strength) mod 512` and is multiplied by `sin[s]` (real) and `sin[s + 128]`
(imaginary). The 4096-entry sine table is indexed modulo 512, so both multipliers lie in [0, sin π/4] (reference quirk,
reproduced). μ starts at a random value in [0, 100) at note-on and advances by `GenMod³/100` per rebuild, wrapping at 100.

### Unison playback

```mermaid
flowchart LR
    f["freq × octave (2^(4 − round(8p)))<br/>or fixed 1000·GenFreq² Hz"] --> det["f₁ = f + 10·detune³<br/>f₂ = f − 10·detune³<br/>× M(freq) / 44100"]
    det --> l1["layer 1: phase L/R"] & l2["layer 2"] & ln["… layer U"]
    l1 & l2 & ln --> rd["wave[trunc(phase·511)]<br/>(no interpolation)"]
    rd --> dr["× (1 + 32·drive), clip ±1"]
    dr --> vol["× gain ramp (last → target over 512)<br/>linear balance pan"]
    vol --> tmp["tmp L / R"]
```

- U = `round(9p) + 1` layers (1..10). Each layer has a left and a right phase; after each layer `f₁ += spread`,
  `f₂ −= spread`, with `spread = 10·GenSprd⁴/44100`.
- At note-on each layer draws a base phase and a right phase `base + [0, 0.1)` (wrapped into [0, 1); the reference
  lets it start above 1).
- Pan is a linear balance: `L = vol·(1 − 2(pan − .5))` for pan > .5, `R = vol·2·pan` for pan < .5.
- The volume ramps linearly within the block from the previous block's gains. A voice whose volume and previous gains
  are below 0.001 skips the generator and keeps its last gains.

### Noise (`noise.rs`)

Each voice reads the shared 65 536-entry white-noise table from two random offsets (one per channel), scaled by
`NoiseOn·velocity·M(noise)`, silent at ≤ 0.01. With `NoiseBw < 0.99` it is band-passed by its own ladder LP at
`NoiseFreq + bw` and RBJ HP at `NoiseFreq − bw` (resonance 0.05). The noise stage overwrites the voice buffer; the
generator then adds into it.

### Filters (`filter.rs`)

Coefficients are set once per block from `(cutoff, resonance) × M`, clamped to [0, 1] and [0, 0.85]. Filters run only
when their `On` parameter is > 0.5, in the fixed order LP → HP → BP → notch.

| Filter | Cutoff | Structure |
|---|---|---|
| Ladder LP | `20000p² + 30` Hz | 4 two-tap one-pole stages `y = p·(old + in) − k·y`, global feedback `x − r·y₃`, output `y − y³/6`. `f = 2fc/sr`, `k = 3.6f − 1.6f² − 1`, `p = (k + 1)/2`, `r = q·e^(1.386249(1 − p))` |
| RBJ HP / BP | `10000p² + 30` Hz | `synth-core` DF1 `Biquad`; bandwidth law uses log₁₀2 instead of ln 2, so they ring more than the cookbook says (reproduced) |
| Notch | `20000p² + 30` Hz | Zeros at ±ω0 on the unit circle, poles at radius q, heuristic b0; the biquad is fed the previous input (1 sample latency, as in the reference) |

## Modulation

### ADSR (`env.rs`) and LFO (`lfo.rs`)

Both advance once per block.

```mermaid
stateDiagram-v2
    [*] --> Finished
    Finished --> Attack: note-on (value = 0)
    Attack --> Decay: value ≥ 1
    Decay --> Sustain: value ≤ S
    Attack --> Release: note-off
    Decay --> Release: note-off
    Sustain --> Release: note-off
    Release --> Finished: value ≤ 1e-5
```

With `STEP = 0.0005·512`:

| Stage | Per-block update |
|---|---|
| Attack | `v += −log₁₀(0.94·max(a³, 1e-9))·STEP` (linear) |
| Decay | `v += ¼·log₁₀(0.94·max((d·M)³, 1e-9))·STEP · ((0.01 + v − S)/(1 − S))^slope`. With `d·M ≳ 1.02` the log turns positive and decay rises (quirk) |
| Sustain | `S = min(s, 0.99)`; a changed S is chased by the decay step |
| Release | `v += ¼·log₁₀(0.94·max(r³, 1e-9))·STEP · v^slope` |

LFO output is unipolar and multiplicative: `d·shape + 1 − d`, `d = (depth·M(depth))²`. Rate `50p²·512/44100` rad per
block (0 to about 8 Hz). Shapes `round(4p)`: sine `(sin φ + 1)/2`, ramp up, ramp down, square, stepped random
(256-entry table).

### Mod matrix (`modmx.rs`)

The patch stores 10 `(source, amount, target)` slots; only the first 8 run (reference behaviour). A destination's
factor `M(dst)` is the **product** of the results of all slots aimed at it, 1 if none. Every modulated parameter is
`base × M`, so modulation scales rather than adds.

```mermaid
flowchart LR
    subgraph sources
        s1["1 LFO1"]
        s2["2 LFO2"]
        s3["3 ADSR1"]
        s4["4 ADSR2"]
        s5["5 mod wheel"]
    end
    subgraph slot["slot i (i < 8)"]
        r["result = amount × source × own_i"]
    end
    sources --> slot
    slot --> d["M(dst) = Π results aimed at dst"]
    d --> gen["generator:<br/>1 bandwidth, 2 damp, 3 harmonics, 4 scale,<br/>5 volume, 6 freq, 7 pan, 8 detune,<br/>9 spread, 10 drive"]
    d --> nf["11 noise, 12–13 LP cut/res,<br/>14–15 HP cut/res, 16–17 BP cut/Q,<br/>18–19 notch cut/Q"]
    d --> me["20–21 ADSR1/2 decay,<br/>22–29 slot 1–8 own amount,<br/>30–31 LFO1/2 depth"]
    me -.->|"feeds back next block / later slot"| slot
```

- Source `round(14p)`: 0 none, 1–5 as above, 6+ reserved (inert). Target `round(39p)`.
- Amount: `2p` up to 0.5, then `1 + 9(2p − 1)²` (0..10).
- `own_i` is `M(22 + i)` from the previous block: slots can modulate each other's depth.
- Slots are evaluated in order, and each source is computed once per block. A slot that reads `M` (LFO depth, ADSR
  decay) sees this block's results of earlier slots and the previous block's results of later ones. This one-block
  slot delay is the reference's.

## Effect chain (`fx/`)

```mermaid
flowchart LR
    in["instrument bus L/R"] --> s1["slot 1"] --> s2["slot 2"] --> s3["…"] --> s10["slot 10"] --> out["out"]
    p["Effect1..10: round(10p)<br/>0 none · 1 distortion · 2 delay · 3 chorus<br/>4 flanger · 5 reverb · 6 formant · 7 EQ · 8–10 none"] -.-> s1 & s2 & s10
```

- All slots share the instrument's effect parameters (two delay slots sound alike).
- A slot type change creates a fresh zeroed instance from the instrument's effect pool. The pool is a bump allocator,
  so a replaced instance's memory is not reused; `Chain::need(patch)` sizes the pool for the patch's initial kinds. A
  kind that is not compiled in (`tf-fx-<kind>`) or no longer fits acts as none.
- Chorus control runs once per 512 frames; the other effects run per sample.

| Kind | Params | Algorithm |
|---|---|---|
| 1 Distortion | DistAmnt | Stateless: `sign(x)·(trunc(min(\|x\|, 1)·32767)/32768)^(1 − amount)` |
| 2 Delay | DelayL, DelayR, DelayDc | Two rings up to 1 s: `len = trunc(44100·trunc(1000p)/1000)`; `b = k(x + oldest)`, write `b`, `y = x + b`. The zero-latency copy makes dry ×(1 + k) and echoes k², k³… (quirk kept) |
| 3 Chorus | ChrsRate, ChrsDep, ChrsGain | 6 rings (3 per side in series), `y = x + 0.7g·x[n − d]`; per block `d = clamp(1 + 9·depth·(sin φ + 0.5), 1, 10)` ms (flat spot where it clamps); random start phases |
| 4 Flanger | FlangLfo, FlangFreq, FlangAmp, FlangWet | 2 rings of 4096; `y = clip(x − wet·ring.at(Δ), ±1)` written back; L/R sweep phases from a bouncing counter. FlangLfo = 0: L = R with a 480 s period |
| 5 Reverb | RevRoom, RevDamp, RevWet, RevWidth | Freeverb on `synth-core` `Comb` ×8 + `AllPass` ×4 per side (right +23): in `(L + R)·0.015`, fb `0.7 + 0.28·room`, damp `0.4p`; wet `3·wet` split by width, dry `2(1 − wet)` |
| 6 Formant | FrmtType, FrmtWet | 10th-order all-pole filter (musicdsp.org #110 coefficients), f64 state, vowel `trunc(4p)` A/E/I/O/U; `y = x(1 − w) + filtered·w` |
| 7 EQ | EqLow, EqMid, EqHigh | 3-band split with 4 one-poles at 880 Hz and 4 at 5 kHz per side; `out = l·gL + m·gM + h·gH`, gain `2p` up to .5, then `1 + 10(2p − 1)²` |

## Patch: the 112 parameters

All values are normalized 0..1 (`.tfm` stores `trunc(100p)` as a byte).

| Range | Group | Parameters |
|---|---|---|
| 0 | Output | Gain: `(2g)²` up to .5, then `1 + 20(g − .5)` (0..11) |
| 1–16 | Generator | Bandwidth, Harmonics, Damp, Mod (phase stage), Volume, Pan, Slop, Octave, Glide, Detune, Freq (0 = key-tracked), Poly, Drive, Unison, Spread, Scale |
| 17–19 | Noise | Amount (NoiseOn), Freq, Bandwidth |
| 20–25 | LP, HP | On, Cut, Res each |
| 26–30 / 31–35 | ADSR 1 / 2 | Attack, Decay, Sustain, Release, Slope |
| 36–39 / 40–43 | LFO 1 / 2 | Rate, Depth, Shape, Sync |
| 44–73 | Mod matrix | 10 × (Source, Amount, Target); slots 9–10 inert |
| 74–83 | Effect slots | Effect1..Effect10 kind |
| 84–103 | Effect params | DistAmnt, ChrsRate, ChrsDep, DelayL, DelayR, DelayDc, RevRoom, RevDamp, RevWet, RevWidth, FlangLfo, FlangFreq, FlangAmp, FlangWet, ChrsGain, FrmtType, FrmtWet, EqLow, EqMid, EqHigh |
| 104–105 | Bend | Up, Down range: `trunc(12p + 1)` semitones |
| 106–111 | BP, notch | On, Cut, Q each |

`params::NAMES` keeps the public names for preset compatibility (index 10 really is `GetDetn`).

## Songs

### `.tfm` and `TfmPlayer`

```text
u16 instrument count I (≤ 32), u16 tempo, I × u16 event counts,
"INST", I × 112 u8 params (p = byte/100),
"SONG", per instrument: u16 row deltas[], u8 notes[], u8 velocities[],
"ENDS"   (bytes after it ignored)
```

```mermaid
flowchart LR
    tfm[".tfm bytes"] --> parse["Tfm::parse<br/>(zero-copy, bounds-checked)"]
    parse --> tp["TfmPlayer::new(song, pool, offset)"]
    tp --> ins["Instrument × I"]
    clock["block clock<br/>t += 512/44100 (f32)"] --> win["events with time in [t, t + 512/sr)<br/>time = row·60/(tempo·4) + offset"]
    win -->|"fired at block start;<br/>note 0 ignored, vel 0 = off"| ins
    ins --> f32o["render_f32: plain sum"]
    ins --> i16o["render_i16: bus += inst × 11;<br/>× 2000·volume (0.1); clamp; trunc"]
```

Rows are 4 per beat. `seek` sends all-notes-off and moves each instrument's cursor to the first event at or after the
target; `mute(i)` stops dispatch (tails still render); `length` skips empty instruments.

### On a `synth-mix` track (`device.rs`)

```mermaid
sequenceDiagram
    participant P as synth-core Player (100-frame sub-blocks)
    participant D as TfInstrument (kind 32)
    participant I as Instrument
    P->>D: note_on(note, vel, offset) up to 511 frames early (lookahead)
    D->>D: queue event, placed at frame = ctx.frame + offset on the next run
    P->>D: run(io, 100, ctx)
    loop each frame f of the call
        alt f enters a new absolute 512-frame block b
            D->>I: fire queued events with frame < (b + 1)·512
            D->>I: render(buf) one 512 block
        end
        D->>P: io.main[f] = buf[f mod 512]
    end
```

- Kind 32 (instrument): chunk = up to 112 f32 params + size. Blocks sit on the absolute song grid (`ctx.frame / 512`),
  so output is bit-identical to the `.tfm` player and independent of the caller's block size.
- Kinds 33–39: each effect as a standalone device with only its own parameters (`effect_params(k)`), so a
  WaveSabre track can end in a Tunefish reverb. Chorus keeps its 512-frame control grid across calls.
- Mixed songs load through `synth_core::mix::Either<WsDevice, TfDevice>`.

## Memory and tables

- Voices come from the caller pool; each instrument carves an effect pool of `Chain::need(patch)` bytes. Delay needs
  2 × 44 100 floats, reverb about 100 KB, chorus and flanger little. `TfmPlayer::required_pool` sizes a whole song.
- `tables()` is a once-initialised static built from a fixed seed: `sin[4096]`, `exp[4096]` (`e^(−5i/4096)`),
  `rho[512]`, `lfo_noise[256]`, `white[65536]`, `note_hz[128]`.

## Quirk policy

Reproduced because they shape the sound: phase-stage table range, 8 live mod slots, block-rate control, decay that can
rise, log₁₀2 HP/BP bandwidth, linear-Hz bend, delay zero-latency copy, chorus flat spot, flanger counter flip, effect
auto-sleep after 2 s, flush-to-zero.

Fixed because they are undefined behaviour with no audible role: song length with empty instruments, right-phase
overrun at note-on, unwrapped chorus/flanger phases, unbounded flanger counter, uninitialised state, entropy seeding.

## Verification

- Unit tests per module against analytic results (`src/tests.rs`), self-goldens (`tests/golden.rs`, `tests/golden.txt`),
  `.tfm` vs shared-track bit identity (`tests/tfm.rs`, `tests/device.rs`).
- `tools/tf-parity`: reference build outside the repo, metrics from `tools/synth-parity`. no_remorse and tf4testsong
  pass with differences of the same order as the reference's own run-to-run spread; 12.9× real time.
- Rendering: `cargo run -p tfsynth --example render -- <song.tfm> <out.wav> [tail-s]`.
