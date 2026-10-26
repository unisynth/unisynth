# WaveSabre engine (`crates/wavesabre`)

A `no_std` f32 Rust port of the 13 WaveSabre devices (Logicoma, logicomacorp/WaveSabre @ 9245fb8, MIT; the upstream
notice is kept in `crates/wavesabre/LICENSE-WaveSabre`). It runs on [`synth-core`](synth-core.md): devices implement
`Device`, instruments use `VoicePool` and `Events<256>`, and songs play through `synth_core::mix::Player`.

The port reproduces upstream numerics on purpose: the 512-entry FastSin table, the `RandFloat` random law, the
`2^(dB/6)` "dB" law, the f32/f64 mix and the expression order. Upstream undefined behaviour (unchecked indices, empty
sample chunks, under-sized chunks) is guarded instead of reproduced. 12 of 16 parity songs are bit-identical with a
native upstream build; the rest are ≥ 100 dB SNR (`research/softsynth-research.md`).

Per-file record: `crates/wavesabre/AGENTS.md`. Spec: `openspec/specs/ws-synth/spec.md`.

## Device set

| Kind | Device | Type | Params | Feature | One line |
|---:|---|---|---:|---|---|
| 0 | Falcon | instrument | 32 | `ws-falcon` | 2-operator phase-modulation synth |
| 1 | Slaughter | instrument | 42 | `ws-slaughter` | 3 BLIT saw/pulse oscillators + noise → SVF |
| 2 | Thunder | instrument | 0 | `ws-thunder` | Deprecated: plays a GSM sample once at unity pitch |
| 3 | Scissor | effect | 6 | `ws-scissor` | Waveshaper with oversampling |
| 4 | Leveller | effect | 14 | `ws-leveller` | 5-band EQ |
| 5 | Crusher | effect | 3 | `ws-crusher` | Bit depth + sample-rate reduction |
| 6 | Echo | effect | 9 | `ws-echo` | Tempo-synced stereo delay |
| 7 | Smasher | effect | 7 | `ws-smasher` | Peak compressor with sidechain |
| 8 | Chamber | effect | 6 | `ws-chamber` | 8-line cross-fed delay reverb |
| 9 | Twister | effect | 9 | `ws-twister` | Flanger / phaser |
| 10 | Cathedral | effect | 8 | `ws-cathedral` | Freeverb |
| 11 | Adultery | instrument | 28 | `ws-adultery` | Sampler over GM.DLS waves |
| 12 | Specimen | instrument | 26 | `ws-specimen` | Sampler over a GSM 6.10 sample in the chunk |

`ws-all` (default) enables everything. Every single-device build compiles warning-free, so a compo build links only the
devices its song uses.

## Crate structure

```mermaid
flowchart TB
    dev["device.rs<br/>WsDevice enum (ws_devices! macro)<br/>impl Kinds + Device"]
    subgraph shared["engine-internal shared code"]
        helpers["helpers.rs<br/>FastSin table, WsRng, curves"]
        synth["synth.rs<br/>Synth&lt;V&gt;, Envelope, VoiceBase, Common, WsCx"]
        svf["svf.rs<br/>Chamberlin SVF, 2× oversampled"]
        wsbq["biquad.rs<br/>WsBiquad (RBJ on FastSin)"]
        sampler["sampler.rs<br/>SamplePlayer, SampleVoice, Sampler"]
    end
    gsm["crates/gsm<br/>GSM 06.10 decoder (libgsm port)"]
    subgraph inst["instruments"]
        falcon["falcon.rs"]
        slaughter["slaughter.rs"]
        thunder["thunder.rs"]
        specimen["specimen.rs"]
        adultery["adultery.rs"]
    end
    subgraph fx["effects"]
        scissor["scissor.rs"]
        leveller["leveller.rs"]
        crusher["crusher.rs"]
        echo["echo.rs"]
        smasher["smasher.rs"]
        chamber["chamber.rs"]
        twister["twister.rs"]
        cathedral["cathedral.rs"]
    end
    core["synth-core<br/>VoicePool, Events, Ring, Comb, AllPass, Biquad"]
    dls["crates/dls<br/>GM.DLS reader"]

    dev --> inst & fx
    falcon & slaughter --> synth
    specimen & adultery --> sampler --> synth
    thunder & specimen --> gsm
    adultery --> dls
    slaughter & sampler & echo & chamber & twister & cathedral --> svf
    leveller --> wsbq
    synth --> core
    echo & chamber & smasher & twister & cathedral --> core
    synth & falcon & slaughter --> helpers
```

## Loading a device

```mermaid
flowchart LR
    rec["DeviceRec { kind, chunk }"] --> k{"kind enabled?"}
    k -- no --> err["LoadError::UnsupportedKind"]
    k -- yes --> init["helpers::init()<br/>(once: 513-entry sine table)"]
    init --> new["T::new(chunk, env, pool, rng)"]
    new --> pb["param_bytes(kind, chunk)<br/>Specimen: after the sample<br/>Thunder: none<br/>others: whole chunk"]
    pb --> sp["set_params: (size − 4)/4 LE f32 → set_param(i, v)"]
```

A chunk shorter than the parameter count leaves the rest at defaults; a chunk under 4 bytes sets nothing.

## Shared numerics (`helpers.rs`)

| Helper | Definition |
|---|---|
| `fast_sin(x)` | 512-entry table + linear interpolation. Index = 9 bits of the mantissa of `frac(x/2π) + 1`, fraction = the next 23 bits |
| `square35(φ)` | `sin(3φ)/3 + sin(5φ)/5` (Falcon's waveform blend) |
| `WsRng` | `seed = seed·0x15A4E35` (wrapping i32), output `(seed % 255)/255`, seed 1, range ≈ [−1, 1] |
| `note_to_freq(n)` | `440 · 2^((n − 69)/12)` |
| `scalar_to_env_value(p)` | Envelope time in ms: `p²·5000 + 1` |
| `volume_to_scalar(v)` | `(0.4v)²` |
| `db_to_scalar(db)` | `2^(db/6)` (not `10^(db/20)`) |
| `param_to_frequency(p)` | `20 + 19980·p²` Hz |
| `param_to_q(p)` | `p < .5`: `0.33 + 0.67·2p`; else `1 + 11·(2p − 1)` |
| `param_to_db(p, range)` | `(2p − 1)·range` |
| `param_to_unisono(p)` | `trunc(15p) + 1` (1..16) |
| `param_to_vibrato_freq(p)` | `(p² + 0.1)·70` Hz |
| pan | `L = √(1 − pan)`, `R = √pan` |

## Instrument core (`synth.rs`)

Falcon, Slaughter, Specimen and Adultery all build on `Synth<V>`: a 256-voice `VoicePool` (`Steal::None`) plus
`Events<256>` and the `Common` settings.

```mermaid
flowchart TD
    subgraph Synth["Synth&lt;V&gt;"]
        ev["Events&lt;256&gt;<br/>note_on / note_off queued with offsets"]
        vp["VoicePool&lt;V&gt; × 256<br/>unison, detune, pan spread, Poly / MonoLegato"]
        common["Common<br/>vib_freq, vib_amount, rise, slide"]
    end
    run["Device::run(io, n)"] --> clr["clear main L/R"]
    clr --> evrun["Events::run(n)"]
    evrun -->|"Step::Event"| vp
    evrun -->|"Step::Render{start,len}"| render["each on-voice in index order:<br/>voice.run(params, common, L[span], R[span])<br/>adds into main"]
    common -.-> render
```

### Common parameters

Every instrument maps these from its own indices: `Unisono` (1–16 voices per note), `Detune`, `Pan` (unison pan
spread), `VibratoFreq`, `VibratoAmount`, `Rise` (`+24·rise` semitones), `VoiceMode` (`(int)v == 1` → mono legato; a
change kills all voices) and `Slide` (legato glide time `10·slide⁴` s).

### Envelope

Times in ms, advanced once per sample by `1000/rate` ms.

```mermaid
stateDiagram-v2
    [*] --> Finished
    Finished --> Attack: trigger()
    Attack --> Decay: pos ≥ attack
    Decay --> Sustain: pos ≥ decay
    Attack --> Release: off()
    Decay --> Release: off()
    Sustain --> Release: off()
    Release --> Finished: pos ≥ release
```

| Stage | Value |
|---|---|
| Attack | `pos / attack` (linear) |
| Decay | `f = (1 − pos/decay)²`; `f + sustain·(1 − f)` |
| Sustain | `sustain` |
| Release | `(1 − pos/release)² · value_at_off` |

A voice turns off when its amplitude envelope reaches `Finished`.

### Voice pitch

`VoiceBase::get_note()` returns the current (possibly sliding) note; the slide moves linearly in semitones over
`rate·10·slide⁴` samples. Each instrument then adds unison detune, rise, vibrato `FastSin(vib_phase)·vib_amount` and
its pitch envelope before converting with `note_to_freq`.

## Falcon (kind 0): 2-operator PM

```mermaid
flowchart LR
    note["note + detune + rise·24<br/>+ vibrato"] --> pe["+ pitch env × amt₁ / amt₂<br/>(±36 semitones)"]
    pe --> f1["f₁ × ratio₁"] & f2["f₂ × ratio₂"]
    f1 --> ph1["phase₁"]
    f2 --> ph2["phase₂"]
    ph1 --> op1["op1 = (FastSin + Square35·wf₁)(in₁)<br/>× env₁ × 13.25"]
    op1 -->|"feedback fb₁ = fb²/2"| op1
    op1 -->|"feed-forward ff²"| op2
    ph2 --> op2["op2 = (FastSin + Square35·wf₂)(in₂)<br/>× env₂"]
    op2 -->|"feedback fb₂²/2 × 13.25"| op2
    op2 --> m["× volume_to_scalar(master)"] --> pan["equal-power pan"] --> out["main L/R (+=)"]
```

- `in₁ = 2π·phase₁/rate + out₁·fb₁`, `in₂ = 2π·phase₂/rate + out₂·fb₂·13.25 + out₁·ff`.
- Frequency ratio per operator: `1 + floor(coarse·32.99) + ((fine − 0.5)·2)³`.
- One song-rng draw at note-on sets both operator phases. Velocity is ignored.
- The voice ends when operator 2's envelope finishes.

| Index | Param | Index | Param |
|---:|---|---:|---|
| 0 | Osc1 waveform | 17 | Master |
| 1 | Osc1 ratio coarse | 18 | Voices (unison) |
| 2 | Osc1 ratio fine | 19 | Detune |
| 3 | Osc1 feedback | 20 | Spread (pan) |
| 4 | Osc1 feed-forward | 21 | Vibrato freq |
| 5–8 | Osc1 A D S R | 22 | Vibrato amount |
| 9 | Osc2 waveform | 23 | Rise |
| 10 | Osc2 ratio coarse | 24–27 | Pitch env A D S R |
| 11 | Osc2 ratio fine | 28 | Pitch env → osc1 (±36 st) |
| 12 | Osc2 feedback | 29 | Pitch env → osc2 (±36 st) |
| 13–16 | Osc2 A D S R | 30 | Voice mode |
| | | 31 | Slide |

## Slaughter (kind 1): subtractive BLIT

```mermaid
flowchart LR
    note["note + detune + pitch env·amt<br/>+ vibrato + rise·24"] --> o1 & o2 & o3
    o1["osc 1: BLIT<br/>+ coarse/fine"] -->|"× vol₁²"| sum
    o2["osc 2: BLIT"] -->|"× vol₂²"| sum
    o3["osc 3: BLIT"] -->|"× vol₃²"| sum
    rng["song rng"] -->|"× noise²"| sum(("Σ"))
    sum --> svf["SVF (LP / HP / BP / notch)<br/>cutoff = freq + modenv·19980·(2·mod − 1)"]
    modenv["mod envelope"] -.-> svf
    svf --> amp["× amp env × −16·volume_to_scalar(master)"]
    ampenv["amp envelope"] -.-> amp
    amp --> pan["equal-power pan"] --> out["main L/R (+=)"]
```

### Oscillator: integrated band-limited impulse train

Per sample, with `P = rate / (2·f)` (half period in samples):

```text
phase  = (phase + 1) mod 2P
blit1  = sinc(π(phase − P))                 # FastSin(x)/x, 1 near 0
phase2 = (phase + 2P·pw) mod 2P − P
blit2  = sinc(π·phase2)
integral = 0.998·integral + dc·(1 − waveform) + blit1 − blit2·waveform      # dc = −0.498/P
```

`waveform = 0` gives a saw (one impulse train plus DC compensation); `waveform = 1` subtracts a second, shifted train
and gives a pulse of width `pw`. Values in between blend. The leak 0.998 keeps the integrator bounded.

At load, each of the 256 voices draws its 3 start phases from the song rng (upstream voice constructors).
Phases and integrators run on across notes; `reset` restores the load-time phases. Coarse detune is
`floor(c·24.99)` semitones plus fine. Velocity is ignored.

### SVF (`svf.rs`)

Chamberlin state-variable filter, 2× oversampled by feeding the midpoint `(last + x)/2` and then `x`, averaging the two
outputs. `f = 1.5·FastSin(π·fc / 2 / rate)`; `q` is `1 − resonance`. Kind = `(int)(p·3)`: 0 LP, 1 HP, 2 BP, 3 notch.

| Index | Param | Index | Param |
|---:|---|---:|---|
| 0–4 | Osc1 waveform, pulse width, volume, coarse, fine | 24–27 | Mod env A D S R |
| 5–9 | Osc2 (same) | 28 | Master |
| 10–14 | Osc3 (same) | 29–34 | Voices, detune, spread, vib freq, vib amount, rise |
| 15 | Noise volume | 35–38 | Pitch env A D S R |
| 16 | Filter type | 39 | Pitch env amount (±36 st) |
| 17 | Filter freq | 40 | Voice mode |
| 18 | Filter resonance | 41 | Slide |
| 19 | Filter mod amount | | |
| 20–23 | Amp env A D S R | | |

## Sample instruments

### Sources and chunk layout

```mermaid
flowchart LR
    subgraph spec["Specimen / Thunder chunk"]
        h["compressed i32<br/>uncompressed i32"] --> wf["WAVEFORMATEX<br/>(0x31 GSM, mono) + cbSize"] --> blocks["GSM 06.10 blocks<br/>(65 bytes = 320 samples)"] --> params["f32 params (Specimen only)"] --> sz["size i32"]
    end
    blocks --> dec["crates/gsm: GsmDecoder<br/>bit-exact libgsm 1.0.24 port"] --> pcm["f32 samples in the pool<br/>(pcm / 32768)"]
    bank["GM.DLS bytes (Env::bank)"] --> dls["dls::Dls::wave(index)"] --> wave["8/16-bit PCM read in place"]
    pcm --> srcF["Source::F32"]
    wave --> srcW["Source::Wave"]
```

The GSM decoder handles WAV49 framing (a 33-byte and a 32-byte frame per 65-byte block). Sample length is
`min(uncompressed, blocks·320)`; a trailing partial block is ignored.

### Sample voice (`sampler.rs`, shared by Specimen and Adultery)

```mermaid
flowchart LR
    note["note − 60 + detune<br/>+ coarse floor((2c − 1)·12)<br/>+ fine (2f − 1)"] --> pitch["delta = ±2^(n/12)<br/>(sign = direction)"]
    pitch --> sp["SamplePlayer<br/>position, loop, interpolation"]
    sp --> svf["SVF<br/>cutoff + modenv"]
    modenv["mod env"] -.-> svf
    svf --> g["× amp env × velocity/128<br/>× volume_to_scalar(master)"]
    ampenv["amp env"] -.-> g
    g --> pan["equal-power pan"] --> out["main L/R (+=)"]
```

`SamplePlayer` (upstream behaviour):

| Setting | Values |
|---|---|
| Start | `sample_start·(len − 1)`, mirrored when reversed |
| Loop mode `(int)(2v)` | 0 disabled, 1 repeat, 2 ping-pong (out of range → disabled) |
| Loop bounds | From the sample (Adultery: DLS `wsmp` loop, else whole wave) or manual (`loop_start·len`, `loop_length·len`) |
| Interpolation `(int)v` | 0 nearest, 1 linear (repeat mode interpolates across the loop seam), else silence |
| End | Position outside the sample → voice off |

A voice takes the sample at note-on and keeps it until its next note-on, so a later sample change never removes
data from under a playing voice.

| Device | Param map |
|---|---|
| Specimen (26) | Amp ADSR 0–3, sample start 4, loop mode 5, loop start 6, loop length 7, reverse 8, interpolation 9, coarse 10, fine 11, filter type/freq/res/mod 12–15, mod ADSR 16–19, voices 20, detune 21, spread 22, master 23, voice mode 24, slide 25. Defaults: no loop, manual bounds |
| Adultery (28) | Sample index 0 (`(int)v − 1` into the bank's waves), then the Specimen map with "loop boundary" (from sample / manual) inserted after loop mode. Defaults: repeat loop, bounds from the sample. No bank or a missing index → silence |

### Thunder (kind 2)

Deprecated upstream. Plays its GSM sample once from the start at unity pitch, mono to both sides, no gain or envelope;
note-off does nothing. No parameters.

## Effects

All effects process main L/R in place. Smasher is the only one that reads the sidechain.

### Scissor (kind 3): waveshaper

```mermaid
flowchart LR
    xin["x"] --> os{"oversampling<br/>(int)(2v)"}
    os -->|"1×"| d1["distort(x)"]
    os -->|"2×"| d2["avg(distort(mid), distort(x))"]
    os -->|"4×"| d4["avg of 4 at ¼ steps<br/>(linear interpolation from last)"]
    d1 & d2 & d4 --> mix["mix(x, y, dry/wet)"]
```

`distort(v)`: `v/threshold · drive`, then shaper `(int)(2v)`: 0 clip with foldover beyond ±1, 1 `v·π/2`, 2 `v³`; clamp to
±1 and multiply back by threshold. Drive: `< 0.2` → `1 − (0.2 − d)`, else `1 + 5·(5(d − 0.2))²`. Params: drive,
threshold, foldover, dry/wet, shaper, oversampling.

### Leveller (kind 4): 5-band EQ

```mermaid
flowchart LR
    xin["x"] --> hp["HP biquad<br/>(low cut)"] --> p1["peak 1"] --> p2["peak 2"] --> p3["peak 3"] --> lp["LP biquad<br/>(high cut)"] --> g["× master"]
```

Each band is a `WsBiquad` (RBJ coefficients from FastSin/FastCos at f32 `w0`, recomputed on change, on
`synth_core::dsp::Biquad`). A peak at exactly 0 dB is skipped. Gains use `param_to_db(v, 24)`. Quirk reproduced: at
20 Hz the FastCos error exceeds `1 − cos w0`, so the lowest low cut acts near 42 Hz with Q ≈ 2.1. Params: low cut
freq/Q, peak 1–3 freq/gain/Q, high cut freq/Q, master.

### Crusher (kind 5)

Per channel: a hold phase advances by `(1 − horizontal)²`; on wrap it samples the input quantised to steps of
`1/2^((1 − vertical)·15 + 1)`. Output `mix(x, hold, dry/wet)`.

### Echo (kind 6): tempo-synced delay

```mermaid
flowchart LR
    inL["in L"] --> wl(("+")) --> bufL["Ring L<br/>coarse·unit + fine ms"]
    inR["in R"] --> wr(("+")) --> bufR["Ring R"]
    bufL --> fl["SVF low + high cut"] --> dL["delayed L"]
    bufR --> fr["SVF low + high cut"] --> dR["delayed R"]
    dL -->|"(1 − cross)·fb"| wl
    dR -->|"cross·fb"| wl
    dR -->|"(1 − cross)·fb"| wr
    dL -->|"cross·fb"| wr
    dL --> oL["mix(in L, delayed L, dry/wet)"]
    dR --> oR["mix(in R, delayed R, dry/wet)"]
```

`unit = 120/tempo/8·1000` ms (a quarter beat; read from `Ctx::tempo` on every call), coarse `trunc(16v)` units, fine
`trunc(200v)` ms. Capacity is sized at load for 16 units + 200 ms at the song tempo. Params: left coarse, left fine,
right coarse, right fine, low cut, high cut, feedback, cross, dry/wet.

### Smasher (kind 7): compressor

```mermaid
flowchart LR
    main["main L/R × input gain"] --> ring["2 ms Ring<br/>(lookahead delay)"] --> g["× gain"] --> out["main L/R"]
    key{"sidechain?"} -->|no| kmain["key = main"]
    key -->|yes| kside["key = SIDE_L/R"]
    kmain & kside --> lvl["level = max(|kL|, |kR|) × input gain"] --> peak["peak follower<br/>attack/release per ms"] --> gc["gain = out gain ×<br/>(thr + (peak − thr)/ratio)/peak<br/>when peak > thr"] --> g
```

Threshold `param_to_db(v/2, 36)`, ratio `18v² + 2`, attack `env_ms(v)/5`, release `env_ms(v)`, input and output gain
`param_to_db(v, 12)`, all through the `2^(dB/6)` law. The ring is written before it is read, so the audio path is
delayed by the 2 ms buffer.

### Chamber (kind 8): delay-network reverb

```mermaid
flowchart LR
    inp["in L/R"] --> pre["optional pre-delay ≤ 500 ms"] --> cut["SVF low + high cut"]
    cut -->|"L"| l4["lines 4–7 (L)"]
    cut -->|"R"| l0["lines 0–3 (R)"]
    l0 -.->|"line j reads line 7 − j"| l4
    l4 -.-> l0
    l0 & l4 --> sum["Σ (dry·(1 − dw) + line·dw) / 4"] --> out["out L/R"]
```

Eight `Ring` lines of 7, 21, 17, 13, 3, 11, 23, 31 ms × mode multiplier (1, 5, 10). For `j` in 0..8, line `j` is
written with `filtered + line[7 − j].oldest()·fb` (lines 0–3 feed R, 4–7 feed L), so each channel's lines are fed by the
other channel's. Feedback `0.5 + 0.5v`. Mode is clamped to 0..2 (upstream indexes the table unchecked). Params: mode,
feedback, low cut, high cut, dry/wet, pre-delay.

### Twister (kind 9): flanger / phaser

```mermaid
flowchart TD
    lfo["vibrato LFO: FastSin(φ)·amount"] --> pos["pos = clamp(amount ± m)<br/>spread: mono / full invert / mod invert"]
    pos --> t{"type (int)(3v)"}
    t -->|"0 / 1"| fl["flanger: Ring read at 2 + 132·pos (fractional)<br/>→ SVF cuts; write in ± out·fb"]
    t -->|"2 / 3"| ph["phaser: 6 first-order allpasses<br/>a1 = (1 − pos)/(1 + pos); in ± last·fb<br/>→ SVF cuts"]
    fl & ph --> mix["mix(in, out, dry/wet)"]
```

Types 0/2 use positive feedback, 1/3 negative. One-second ring per channel. Params: type, amount, feedback, spread, vib
freq, vib amount, low cut, high cut, dry/wet.

### Cathedral (kind 10): Freeverb

```mermaid
flowchart LR
    inp["(L + R) × 0.015"] --> pre["pre-delay ≤ 500 ms"]
    pre --> cl["8 combs L<br/>1116 … 1617"] --> al["4 allpasses L<br/>556, 441, 341, 225"]
    pre --> cr["8 combs R<br/>(+23 samples)"] --> ar["4 allpasses R (+23)"]
    al --> fL["SVF low + high cut"]
    ar --> fR["SVF low + high cut"]
    fL & fR --> w["width mix<br/>wL = L·w1 + R·w2<br/>wR = R·w1 + wL·w2"]
    w --> o["mix(dry, wet, dry/wet)"]
```

Shared `synth_core::dsp::{Comb, AllPass}` with stock 44.1 kHz tunings; allpass feedback fixed 0.5, comb feedback =
room, damp = damp. Freeze sets room 1, damp 0, input gain 0. `w1 = width/2 + 0.5`, `w2 = (1 − width)/2`. Quirk kept: the
right output uses the already-mixed left. Params: freeze, room, damp, width, low cut, high cut, dry/wet, pre-delay.

## A typical song graph

```mermaid
flowchart LR
    subgraph t0["track 0: bass"]
        s0["Slaughter"] --> e0["Leveller"]
    end
    subgraph t1["track 1: kick"]
        sp["Specimen"]
    end
    subgraph t2["track 2: pads"]
        f2["Falcon"] --> sm["Smasher (sidechain on)"] --> ch["Chamber"]
    end
    subgraph t3["track 3: reverb bus"]
        cat["Cathedral"]
    end
    subgraph m["track 4: master"]
        lv["Leveller"] --> sc["Scissor"]
    end
    t1 -->|"recv ch 2 (sidechain)"| t2
    t0 & t1 & t2 -->|"recv ch 0"| m
    t2 -->|"recv ch 0 × send"| t3 --> m
```

The player orders tracks so every sender renders before its receivers; the last track is the master.

## Verification

- `crates/wavesabre/tests/golden.rs`: FNV-1a hashes of synthetic songs covering every device (`tests/songs.rs`),
  rendered in odd 777-frame chunks; reset = fresh load; interleaved songs equal separate renders; pool one byte short
  → `OutOfMemory`.
- `tools/ws-parity`: native upstream build vs ours per device (SNR ≥ 60 dB).
- `tools/synth-parity`: whole songs (BS.1770 loudness, octave spectrum, onsets).
- Rendering: `cargo run -p wavesabre --example render -- <song.bin> <out.wav> [bank.dls]`.
