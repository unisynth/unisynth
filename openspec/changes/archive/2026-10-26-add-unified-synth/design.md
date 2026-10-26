# Design

## Context

Current state (see `docs/synth/` for diagrams):

- `crates/synth-core` holds the `Device` contract (4 channels, offsets, lookahead, `reset`), `Events<N>`, `VoicePool`
  (`Steal::{None,Oldest}`, `Mode::{Poly,MonoLegato}`, unison spread), the DSP primitives (`Comb`, `AllPass`, `Ring`,
  DF1 `Biquad`, `OnePole`), `Pool`, `Rng`, `denormal::Guard` and the `synth-mix` song blob + `Player` (100-frame
  sub-blocks, automation once per sub-block, kinds through a `Kinds` registry, `Either<A, B>` for mixed songs).
- `crates/wavesabre`: 13 devices, per-sample control, 256 voices without stealing, FastSin table, `WsRng` song-wide
  draws, `2^(dB/6)` dB law. 12 of 16 device songs are bit-identical with upstream.
- `crates/tfsynth`: one 112-parameter instrument, 512-frame block control, 16 voices with oldest stealing, a
  multiplicative mod matrix and 10 insert effects, 44.1 kHz constants, its own xorshift.
- Measured: WaveSabre all devices 27.6 KB packed (Slaughter only 14.3 KB); Tunefish 18.9 KB (no effects 14.8 KB).
  Elysian renders at 5.6× real time and no_remorse at 12.9×.

Constraints from the user (proposal "Why"): perceptual parity, not bit parity. Fixed slots, one control rate, no hard
size target, and the legacy crates retire only after a passed parity report and an A/B sign-off.

## Goals / Non-Goals

**Goals:**
- Every legacy instrument and effect can be expressed in the unified set and sounds close after conversion.
- Converting is mostly mechanical: module parameters copy, and only cross-cutting parameters (pitch, level, LFO rate,
  routes) need mapping.
- Output is independent of caller buffer size, player sub-block size (without automation) and call split.
- Unused modules cost no code (features) and no memory (per-patch sizing).

**Non-Goals:**
- Bit parity with either legacy engine, or with upstream WaveSabre / Tunefish.
- Preserving legacy quirks that exist only because of block rate or engine plumbing: Tunefish effect auto-sleep,
  the 100-frame random-draw coupling, Tunefish linear-Hz pitch bend. Songs never carry bend.
- A new authoring UI. Patches come from the converter or are hand-written in tests until the editor work.

## Decisions

### D1. Crates

```text
crates/gsm            no_std  GSM 06.10 WAV49 decoder + WaveSabre sample-chunk parser (moved from wavesabre/src/gsm.rs)
crates/unisynth       no_std  unified instrument (kind 64) + unified effects (65–73), UDevice registry
crates/synth-convert  std     legacy → unified converter, feature analyser, bin `synth-convert`
tools/unisynth-parity bun     convert + render both ways + tools/synth-parity metrics + committed report
```

`crates/wavesabre` switches to `crates/gsm` and is otherwise frozen, and so is `crates/tfsynth`. They are the parity
oracles. Algorithms are **ported** into `unisynth` (moved and adapted), not shared with the legacy crates. Sharing
would tie the new crate to code that is about to be deleted. The short duplication ends with `retire-legacy-synths`.
Generic primitives stay in `synth-core`, and `unisynth` uses them; the tf-synth "no duplicated primitives" check is
extended to `unisynth`.

*Alternatives:* putting the instrument into `synth-core` was rejected, because the D2 rule keeps engine algorithms
out of the core. Refactoring the legacy crates to call `unisynth` modules was rejected: it would break their role as
independent oracles.

### D2. Modules keep their legacy parameter laws; only cross-cutting parameters are new

Every module type keeps the normalized → physical laws of the legacy code it comes from:

- band-limited saw/pulse: Slaughter laws;
- phase-mod operator: Falcon laws;
- wavetable: the Tunefish `Gen*` laws;
- sampler: Specimen/Adultery laws;
- envelope modes: WaveSabre ms laws or Tunefish rate laws;
- every effect mode: the legacy effect's whole parameter list.

New laws exist only where the two engines meet in one place. Constants fixed in task 1.3 (`crates/unisynth/src/law.rs`):

| Parameter | Law (normalized p) | Why |
|---|---|---|
| Filter cutoff (all filter types) | `20 + 19 980·p²` Hz | WaveSabre's law; Tunefish's ladder/notch (`30 + 20 000p²`) and RBJ (`30 + 10 000p²`) are close or a constant factor √2 in p, so Tunefish's `p·M` scaling stays a scale. The converter re-maps through the inverse |
| Resonance / Q | per filter type, legacy law of that type | the types behave differently anyway |
| Voice transpose | `(2p − 1)·24` semitones, continuous | WaveSabre rise (`24p`) and general transposition |
| Osc coarse / fine / octave | whole semitones `±24` / `2p − 1` semitones / whole octaves `±4` | Slaughter coarse `floor(24.99d)` and fine, Specimen `floor((2c − 1)12)` and `2f − 1`, Tunefish octave `2^(4 − round 8p)` (p flipped) |
| Osc ratio, ratio fine | `1 + floor(32.99p)`, `(2p − 1)³` | Falcon's ratio law, available to every oscillator type |
| Fixed frequency | `1000·p²` Hz | Tunefish |
| Levels (voice master, osc gain, noise level) | `16·p²` (0..16, unity at p = 0.25) | Falcon's ×13.25 operator and Tunefish's 0..11 curve fit; WaveSabre `(0.4v)²`, `p²` re-map through the inverse |
| Phase-mod send / feedback | `4p²` / `8p²` radians per unit of the slot's output | Falcon `ff²` (≤ 1), `fb²/2` (≤ 0.5) and `13.25·fb²/2` (≤ 6.6) |
| Pan | equal power `(√(1 − p), √p)` | WaveSabre; Tunefish's linear balance differs by ≤ 3 dB at the centre, absorbed by the converter's gain |
| LFO rate | `0.01·10^(4p)` Hz (0.01–100 Hz) | covers WaveSabre vibrato (7–77 Hz) and Tunefish LFOs (0–8 Hz) |
| Slide / glide time | `10·p⁴` s | WaveSabre's slide law, copied |
| Start-phase spread | `10^(6p − 2)` samples of the oscillator's own period, the whole turn at `p = 1` | WaveSabre draws its start phases in samples, not turns (Slaughter ±2π samples, Falcon a fraction of one), so its unison voices start almost in phase; the default keeps the full random turn |
| Envelope time (WaveSabre mode) | `p²·5000 + 1` ms | WaveSabre's law, copied (Tunefish mode: rate laws of the legacy envelope) |
| Scale amount | Tunefish `2p` ≤ 0.5, then `1 + 9(2p − 1)²` | Tunefish's law, copied |
| Offset amount | signed square `sign(x)·x²·range`, `x = 2p − 1`, range per target (pitch ±36 st, cutoff ±19 980 Hz) | fine steps near zero for vibrato depths, full range at the ends |
| Noise band | HP and LP cutoff in the cutoff law | Tunefish's centre ± bandwidth is converted to two cutoffs |

**Criteria, checked by `params/tests.rs`.** *Coverage:* every legacy range (cutoffs, vibrato and LFO rates, rise,
coarse, fine, octave, ratio, envelope and slide times, gain, feedback, send, amounts) maps to p within [0, 1] and
back. *Resolution:* a law that is a legacy law (cutoff, envelope time, slide time) is exactly as fine as the legacy
byte; the cutoff law is also finer than Tunefish's byte/100 at every frequency. For laws that are new for a quantity
(LFO rate, gain, transpose, offset amounts) "no coarser than legacy" would force a narrower range, so they carry
perceptual bounds instead: LFO rate ≤ 4 % per byte, gain ≤ 0.7 dB per byte down to −40 dB under the maximum,
transpose ≤ 0.2 semitones per byte, pitch offset ≤ 0.1 semitone per byte up to ±1 semitone (vibrato), ≤ 0.35 up to
±12, ≤ 0.6 up to ±36. Byte resolution only matters for automation; chunk values are f32.

Because module parameters copy, conversion mostly moves indices. The final constants live in one table in
`unisynth/src/params.rs` (task 1.2), which the converter imports. The table must cover both engines' ranges, and its
byte-quantized resolution must not be coarser than the legacy laws in their common range.

*Alternative:* one fresh law for every parameter (cleaner patches). Rejected: every parameter and every automation
lane would need a non-linear remap, and Tunefish Scale modulation would change its curve.

### D3. Patch layout and chunk

A flat parameter array of 180 values in blocks:

```text
voice   16  master, pan, capacity, steal, unison, uni-detune, uni-spread, mode, slide, glide, transpose,
            velocity sensitivity, start-phase spread, one-shot, noise level, env1-at-output
osc ×3  22  type, key-track/fixed, coarse, fine, octave, ratio, ratio fine, fixed freq, gain, mix, pan,
            pm-out, feedback, shape[9] (meaning by type: PM wave blend; BLIT waveform + pulse width;
            wavetable Gen* + layers/detune/spread; sampler start, loop, reverse, interpolation, source)
noise    3  band on, hp cutoff, lp cutoff
filt ×4  3  type, cutoff, resonance
env ×4   6  mode, attack, decay, sustain, release, slope
lfo ×2   4  shape, rate, depth, sync
mod ×12  4  source, target, amount, operation
smp      3  sample index of osc slot 1–3     (raw, structural; indices 177–179)
ctl      3  wheel, bend, bend range          (appended for the DAW, indices 180–182)
ext      1  noise width                      (appended for the demo songs, index 183; COUNT = 184)
```

Chunk v1, little endian:

```text
u8 version = 1
u8 n, n × { u8 index, f32 value }        # only values that differ from DEFAULTS, indices strictly increasing
u8 s,  s × { u32 size, GSM sample chunk } # WaveSabre Specimen sample layout: header + WAVEFORMATEX + blocks, no tail
(end of chunk: no further bytes)
```

Decoder rules: unknown version, `index ≥ COUNT`, an index not greater than the previous one, a non-finite value, a
normalized value outside 0..1, a sample entry with size 0 / past the end / a bad GSM header / bytes after its
compressed data, a GSM sampler index past `s`, and any bytes after the last entry are `Malformed` or `BadChunk`.
The three sample indices (177–179) are the only raw values: finite, 0..65 535, read as `(int)v`, 1-based
(0 = none; DLS uses the legacy `(int)v − 1`). Strictly increasing indices make the encoding canonical. The worst case
(all 180 pairs, 903 bytes) is larger than a dense f32 array (708 bytes), which is accepted because GSM samples dominate
any chunk that matters.

**Why the sample index has its own indices.** It first lived in `shape[8]` of each oscillator slot, which the wavetable
also uses as its normalized layer spread. One index cannot be both raw and normalized, so a Tunefish song's spread was
rejected by the song validator and ignored by `set_param` as structural. Found when the Tunefish reference songs were
saved as `song/1`. The three raw indices moved to 177–179; `shape[8]` is now an ordinary shape value for every type
(unused by the sampler). Rendering is unchanged: every golden is byte-equal.

**Controller block (added for `add-synth-daw`).** Wheel, bend and bend range are appended at indices 180–182 instead
of joining the voice block, so every existing index, default, chunk and golden stays valid; the decoder's bound moves
from 180 to 183 (then 184 with the noise width, below). The `synth-mix` blob has no controller events, so the DAW carries wheel and bend as ordinary
automation of these parameters (block-start interpolation at the player sub-block). `mod_wheel` stays as the live input
and writes index 180 (a reset restores the loaded controller values). Bend is added to the voice's tuning (transpose),
so it moves held and releasing voices alike and, like transpose, not a fixed-frequency oscillator.

The sampler `source` choice grows a third value, DLS instrument (0 GSM, 1 instrument, 2 wave, so the stored 0.0 and
1.0 keep their meaning), whose index packs `1 + drums·16384 + bank·128 + program` (≤ 32 768, inside the raw 0..65 535
range) so it resolves by `Dls::find(drums, bank, program)` in whichever OS bank is present. The instrument is resolved
once at load and kept as a pool record (`Inst`); each note-on picks `region_for(key, velocity)` and the voice keeps that
region's wave, root key (`unity_note − fine/100`), loop and the wave's rate against the song's. `wsmp` gain is not
applied: the macOS bank holds −20 dB (drums) to −113 dB (piano) there, paired with articulation we ignore.

**Structural parameters** (memory shape): voice capacity, each osc type, each sampler source and index are read from the
chunk only; `set_param` ignores them. Everything else (filter type, envelope mode, LFO shape, mod slots, …) lives
inline in the voice with a fixed size, so automation may change it, but a mid-note type change clicks (the converter
emits discrete automation as steps; D10).

**Mod matrix ids.** Source 0 and target 0 mean "none" (the slot is inert: Scale result 1, Offset 0). The source and
target id tables are assigned in task 2.4 and frozen before task 6.1; the chunk version stays 1 until the first
converted song exists. Tunefish's two dead slots (9 and 10) are dropped by the converter.

**Sizing** (`need`) uses overflow-checked arithmetic on every term (sizes are `u32`, `usize` is 32-bit on i686) and
returns an error rather than wrapping.

The sparse pairs keep chunks small: a Slaughter-like patch sets about 20 values (100 bytes instead of 700). A u8
index caps the layout at 256 values. `Song::parse` already bounds the chunk, and the unified decoder checks the
version, indices, counts and sizes against the chunk length before building anything.

*Alternatives:* dense f32 like the legacy devices (700 B per instrument, mostly defaults); u8 values like `.tfm`
(losing the precision that converted physical values need). The format needs `doubt-driven-review` (task 1.3)
before code depends on it.

### D4. Voice signal flow

```text
osc1 ─┐  y_i = wave_i(φ_i + fb_i·y_i[n−1] + pm_in_i) · gain_i      pm_in_{i+1} = y_i · pm_out_i
osc2 ─┼─ Σ y_i · mix_i · pan_i ─┐
osc3 ─┘                        ├─► filt1 ► filt2 ► filt3 ► filt4 ─► [× env1 · vel] · master ─► main L/R (+=)
noise ────────────────────────┘                                      (only with env1-at-output)
```

- **Where envelope 1 applies.** With env1-at-output on (the default; Slaughter, samplers), envelope 1 and velocity
  scale the voice after the filters. With it off, envelope 1 acts only through matrix routes. Falcon routes it to the
  carrier's gain, so its feedback sees the enveloped signal. Tunefish routes it to the generator gain before the
  non-linear ladder, as `M(volume)` does. Either way, the voice ends when envelope 1 finishes, or earlier when a releasing
  voice has become inaudible (D5).
- **Noise width.** Tunefish's noise has a signal of its own on each channel; WaveSabre's Slaughter adds one noise to
  both. The width parameter (appended at index 183, default 1 = independent) keeps both: the converter sets 0 for
  Slaughter. Found by parity: in a mono sum independent channels read 3 dB lower, which the octave bands showed on
  every hi-hat; per-channel power was already equal.
- The PM topology covers Falcon. For op1: `gain = 13.25` with a Scale route from its envelope, `mix = 0`,
  `pm_out = ff²`, `fb = fb₁²/2` (the `16p²` gain law reaches 13.25). For op2: `gain = 1` with a Scale route from envelope 1, `fb = 13.25·fb₂²/2`. The
  other types use `gain`, `mix` and `pan` and ignore `fb`/`pm_in` where they have no phase input (sampler).
- Unison exists at two levels: voice-level (pool, WaveSabre: whole voices detuned and spread) and layer-level inside a
  wavetable slot (Tunefish: up to 10 phase layers that share the voice's filters).
- A voice ends when envelope 1 finishes or, with one-shot set, when every sampler slot has ended.

### D5. Control block of 32 frames on the absolute grid

Control (envelopes, LFOs, matrix, filter coefficients, wavetable check) runs at absolute song frames that are multiples
of `CONTROL = 32`. At each tick a voice computes the control values for the end of the block and ramps linearly from
the previous values across the block. A voice that starts mid-block computes its start values at its note-on frame and
reaches the grid at the next multiple of 32. So timing depends only on absolute frames, never on the call split.

32 frames (0.73 ms at 44.1 kHz) is below WaveSabre's 1 ms minimum envelope time, so its attacks keep their shape.
Tunefish's 512-frame steps become finer, which perceptual parity allows. Tunefish-mode envelopes express their
per-512-block steps as rates per second (`step·sr/512`), so a curve keeps its timing. The Tunefish wavetable keeps its
rebuild cadence in time: it is checked every 2048 frames after note-on, and rebuilt only when its inputs changed.

**Fine window (found by parity).** WaveSabre's envelopes run per sample, and a 2 ms noise hit sampled every 32 frames
and interpolated linearly lost its peak and gained a tail: Global Groove, a song of nothing else, measured 1.75 dB quiet.
For the first 256 frames after a note-on and after a note-off a voice therefore ticks on the absolute 8-frame grid (a
hit then keeps its energy within about 6 %); the rest of the voice's life stays on 32. The pool tick and the render
spans follow the shortest voice segment, so call-split independence is unchanged. It costs under 1 % of a song.

**Inaudible releases (found by speed).** A releasing voice whose mean output magnitude stays below 2.5e-4 (−72 dBFS)
over 256 frames is freed. Tunefish's release is exponential and the envelope only reports `finished` at 1e-5, so
no_remorse held 33 voices on average against the legacy engine's 19 (which frees a voice when its block level falls
under a threshold); with the rule it holds 19, renders twice as fast and measures the same (loudness −0.21 dB, bands
within 1.7 dB).

*Alternatives:* per-sample control (WaveSabre) was rejected on cost (4 envelopes, 2 LFOs, 12 matrix slots per voice per
sample). 64 frames was rejected because it is coarser than WaveSabre's shortest attack. Per-module rates were rejected
by the user.

### D6. Mod matrix: Scale and Offset

For a target with base normalized value `b`:

```text
Scale  target:  physical = f(b · Π r_s)           (normalized space: cutoff, resonance, pan, shapes, decay, LFO depth)
                physical = f(b) · Π r_s           (physical space: frequency, linear gain, a slot's own amount)
Offset target:  physical = f(…) + Σ r_o           (the target's unit: semitones, Hz, linear gain, normalized)
r_s = amount · source · own_s        r_o = amount · source
```

Which space a Scale factor works in is a property of the target (`modmx::target::scales_physical`), matching Tunefish:
its `p·M` for cutoffs, shapes and pan, but `freq·M`, `vol·M` and `noise·M` on the values. Source values:

- LFO: Scale view `1 − d + d·shape`, Offset view `d·(2·shape − 1)`, `d = (depth·M(lfo depth))²`.
- Envelope: its value, for both operations. Wheel, velocity (`vel/128`) and key (`note/127`) likewise.
- Source ids: 0 none, 1–4 envelopes, 5–6 LFOs, 7 wheel, 8 velocity, 9 key.

Target ids (frozen with chunk v1, `modmx::target`): 0 none, 1 pitch, 2 gain, 3 pan, 4–48 oscillator slot `i` field `f`
(`4 + 15i + f`: pitch, gain, mix, pan, pm-out, feedback, shape 0–8), 49–51 noise level/HP/LP, 52–59 filter cutoff and
resonance per slot, 60–63 envelope decay, 64–65 LFO depth, 66–77 mod slot own amount. A target id ≥ 78 is inert.

- **`own_s`** is the product of the Scale slots aimed at the slot's own amount, as it stood at the end of the previous
  pass (Tunefish's slot-to-slot delay, now one control block).
- **Order.** Slots are evaluated in order. Earlier slots' results of this pass and later slots' results of the previous
  pass are visible, like in Tunefish. An LFO source is read with the depth modulated by what the matrix says so far.
  Envelope decay reads the previous pass.
- **Offset units** come from the target (`target::offset_range`): pitch ±36 st, linear gain ±4, oscillator feedback ±8,
  cutoff ±19 980 Hz, normalized parameters ±1. The clamp happens after the sum, as Slaughter does with `[0, 19 980]` Hz.
- **Legacy routes become slots.**
  - Slaughter/Specimen mod env → cutoff: Offset, amount `19 980·(2m − 1)`.
  - Pitch env: Offset, amount `±36·(2a − 1)`.
  - Vibrato: LFO 1 sine, depth 1, Offset on pitch with amount `vib_amount`.
  - Tunefish slots: Scale with the same source and amount.

### D7. Numerics

- **Sine.** One table, `sin[4096]` f32 (one period), with a linear-interpolation reader for oscillators, LFOs and SVF
  coefficients. The Tunefish phase-stage quirk keeps indexing the raw table modulo 512, as it does now. This replaces
  the WaveSabre FastSin (512 + lerp on f64 mantissa bits) and libm `sin` in modules.
- **Random.** Each unified device owns a xorshift32 that is seeded at load from one draw of the song source, in device
  order. Each voice derives its own stream at note-on (instrument stream → voice seed), so noise and random phases do
  not depend on the render order of other tracks or the sub-block size. This removes WaveSabre's 100-frame step
  coupling.
- **dB.** Instrument levels are linear gains. Effect modes keep their legacy dB law, because it is part of their
  parameter curve (D2).
- **Sample rate.** Every module and effect takes `Ctx::rate`. The Tunefish-derived code, which hard-codes 44 100, is
  rewritten to use it.
- **Float control.** The denormal guard is on for every instrument and effect render, as in Tunefish.

### D8. Effects

`UFx` covers kinds 65–73, with an enum variant per enabled (kind, mode). The mode is parameter 0. The rest is the
legacy parameter list, so conversion copies it and shifts automation indices by one. Code is ported from the legacy
effect and rewired to the shared sine and rate, but it keeps its own algorithm and control grid: Tunefish chorus
stays on an absolute 512-frame grid. Freeverb-based modes share one config-driven core: spread mask, input gain, and
wet/dry law (Cathedral width vs. Tunefish `3·wet`/`2(1 − wet)`). Ring delays use `synth-core::Ring` in both modes'
resize semantics.

The instrument has no insert slots. A Tunefish insert chain runs on the summed instrument output with shared
parameters, which is exactly a chain of track devices after the instrument with copies of those parameters.
`.tfm` songs never change slot kinds mid-song. A `synth-mix` song that automates a Tunefish effect-slot kind
parameter is rejected by the converter with a message.

### Mode names

A mode is named for what it does, not for the engine it came from. Envelopes: `timed` (times in milliseconds) and
`rate` (per-block rate steps). Effects: distortion `shaper`, `power`; EQ `parametric`, `3band`; delay `tempo`, `ms`;
flanger `phaser`, `ring`; reverb `freeverb`, `compact`, `rooms`; one-mode kinds `crusher`, `chorus`, `compressor`,
`formant`. The Cargo features (`us-env-timed`, `us-fx-reverb-rooms`, …), the module ids (`ENV_TIMED`,
`FX_REVERB_ROOMS`, …), the messages and the DAW's mode list use them. Chunk values do not change: a mode is still the
index in its kind, so no song or golden moves.

### D9. Memory and features

- **Sizing.** `UInstrument::need(chunk)` reads the patch: capacity × the per-voice state for the slot types in use
  (a wavetable slot adds about 10 KB per voice of spectrum, work and wave buffers; a PM slot a few floats), plus
  decoded samples. Effects size from their parameters and the song tempo/rate.
- **Instrument features.** `us-osc-pm`, `us-osc-blit`, `us-osc-wt`, `us-osc-sample`, `us-sample-gsm`, `us-sample-dls`,
  `us-noise`, `us-flt-svf`, `us-flt-ladder`, `us-flt-rbj`, `us-flt-notch`, `us-env-timed`, `us-env-rate`.
- **Effect features.** `us-fx-<kind>` plus `us-fx-<kind>-<mode>`, with `us-all` as the default.
- **Load-time checks.** A disabled module fails at load with `LoadError::UnsupportedModule(id)`. This is a new variant
  of `synth-core`'s `LoadError`; adding it is not a requirement change, since `synth-mix` already reports
  unsupported kinds.

### D10. Converter

`synth-convert ws|tfm|patch|features <in> [<out>]`:

- **`ws`** handles `synth-mix` blobs with legacy kinds 0–12 and 32–39. It walks the song with `synth_core::mix::Song`,
  maps each device record, and re-emits tracks, lanes and receives unchanged.
- **`tfm`** decodes with `tfsynth::tfm`. It emits one track per non-empty instrument plus its effects, and a master
  with the `.tfm` output scaling (each instrument received at ×11, master volume `2000·0.1` of the 16-bit full scale).
  Events go at the start of the 512-frame block containing their row time, found with the legacy player's own `f32`
  block clock, as the legacy player fires them.
- **`patch`** turns a Tunefish patch and a note phrase into a song, for the device-level parity of `tfsynth`'s
  synthetic patches.
- **`features`** prints the needed cargo features.

Mapping per legacy instrument:

| Legacy | Unified |
|---|---|
| Falcon | 2 PM slots (D4), env1-at-output off, env1 = op2 env (Scale → slot 2 gain), env2 = op1 env (Scale → slot 1 gain), env3 = pitch env (Offset → slot 1 and slot 2 pitch with their amounts), vibrato LFO (Offset → voice pitch), voice unison/mode/slide |
| Slaughter | 3 saw/pulse slots, noise level², filter 1 = SVF type, env1 = amp, env2 = mod env (Offset → cutoff), env3 = pitch env, vibrato, rise → transpose |
| Specimen / Adultery | sampler slot 1 (GSM sample section / DLS wave index), filter 1 SVF, env1 = amp, env2 = mod env, velocity sensitivity 1 |
| Thunder | sampler slot 1, unity pitch (key track off), one-shot, env1 = instant gate, velocity sensitivity 0 |
| Tunefish | wavetable slot 1 (all `Gen*` copied), noise + band, filters LP/HP/BP/notch in slots 1–4 if On, envelopes (Tunefish mode), LFOs 1/2, mod slots 1–8 as Scale slots **at the same index** (volume → slot 1 gain), slots 9–10 dropped. env1-at-output off; velocity as Scale routes on slot 1 gain and noise level (slots 9, 10). Envelope 1 = the ADSR routed to volume (the other ADSR is envelope 2). If none is routed, envelope 1 is a gate whose release follows the longest envelope in use (the organ-gate rule) and the ADSRs are envelopes 2 and 3. Capacity = `GenPoly`, steal oldest. Insert effects become track devices after the instrument |

- **WaveSabre capacity.** The converter takes the song's maximum overlapping notes × unison, including the amp release
  tail, capped at 64. It warns when the cap cuts voices, since WaveSabre's 256 voices never steal.
- **Start phases.** Slaughter's three oscillators start within ±2π samples of each other (`rand · 2π` is added to a
  phase counted in samples), Falcon's two within 1/44 100 of a turn. The converter sets the voice's start-phase spread
  to 12.57 samples (Slaughter) or the minimum (Falcon). Without it a unison group at one pitch sums 6 dB lower than the
  original and detuned unison stacks are up to 9 dB lower in the bass band, because the unified default spreads the
  start over the whole turn.
- **Vibrato rate.** WaveSabre advances its vibrato phase by `freq` radians per second (the phase is the argument of
  `FastSin`, in radians), so the LFO runs at `freq/2π` Hz (1.1–12 Hz), not `freq`.
- **Level compensation.** Thunder (full level on both sides) gets +√2 on its gain, since the unified pan is equal power
  (−3 dB per side at the centre). Tunefish's linear balance (the louder side at full level) is reproduced exactly for a
  static pan: the pan whose left/right ratio is the square of the balance's, and the gain `√(1 + r²)` (√2 at the
  centre). Noise is unpanned in Tunefish and gets √2.
- **Organ gate.** A Tunefish voice sounds while the key is held and until every envelope that a mod slot reads has
  finished, then gates at once. Without an ADSR on volume, envelope 1 is a gate that does not scale the output: instant
  attack, sustain 1, a release equal to the longest release in use (simulated from the sustain level in whole 512-frame
  blocks, up to 5 s) plus a third of a block, where an abrupt cut has the energy of Tunefish's linear closing fade.
- **Not converted.** `GenSlop` (a random per-voice detune of up to ±8 Hz; the converter warns above 1 Hz); the
  stepped-random LFO sequence; mod slots 9 and 10, which Tunefish never evaluated.
- **Discrete automation.** The converter evaluates the legacy decode of a discrete parameter at every 100-frame step
  and emits two points at the same frame wherever it changes (no ramp through intermediate values). WaveSabre's
  truncating decodes (`(int)(3p)` filter type, `(int)v == 1` voice mode, `(int)(2v)` loop mode) flip at different
  points than the unified rounding, so thresholds are never copied.
- **Automation.** Each point maps p → legacy law → physical → inverse unified law → byte. Points are inserted where the
  interpolated unified value would differ from the legacy curve by more than one byte step. Effect automation is copied
  with index + 1.

### D11. Parity pipeline

```text
legacy song ──► legacy crate render (wavesabre example render, 100-frame step | tfsynth render) ──► ref.wav
     └──► synth-convert ──► unified blob ──► unisynth example render ──────────────────────────► ours.wav
                                                         tools/synth-parity compare ─► report row
```

- **Device level.** Every song in `wavesabre/tests/songs.rs` (via `examples/render_device`), plus every `tfsynth`
  synthetic patch through `synth-convert patch`.
- **Song level.** The six reference songs from the synth lab (`WS_SONG_DIR`, `TF_SONGS`); they are skipped when absent.
- **Report.** `tools/unisynth-parity/report.md` holds metrics, speed and sizes, never audio. It is committed and
  regenerated before the retirement decision. Tolerances are those of the legacy song parity specs (1 dB, 3 dB,
  onsets 100 frames for WaveSabre and 512 for Tunefish), with these definitions found necessary by running it:
  - **Bands** more than 24 dB below the loudest reference band are not compared (as in the in-process tests): between
    two different engines they hold leakage, the sub-bass of envelope edges and the 16-bit noise floor, not the sound.
  - **Onsets** are judged by the lag and correlation of the onset-strength curves (lag within the tolerance,
    correlation ≥ 0.5; the lowest seen on a passing render is 0.57). Peak picking is reported but not judged: for two
    engines the picker keeps a different one of two neighbouring hits in each render (Elysian: 17 of 153 reference
    onsets "missing" at exactly a rhythmic step), and a much cleaner reference stopband hides notes in its own curve.
  - **Random draws.** Global Groove (32 noise bursts of 2 ms) and the five unison patches depend on random numbers the
    engines draw differently (a legacy LCG with a fixed seed against a per-voice xorshift). They are rendered with 8
    seeds of ours; the mean must agree within the usual tolerance plus twice the spread between seeds. Across seeds
    Global Groove's lowest bands move by up to ±11 dB while its loudness moves by ±0.8 dB.
- **Speed** is timed on `[profile.fast]` (release at opt-level 3) for both engines, and on `release` (size) for the two
  floor songs; the legacy figures of the spec were measured on `release`.

## Risks / Trade-offs

- [Control-rate change alters Tunefish envelope and LFO shapes, since Euler steps get finer] → Per-second rate
  conversion keeps the timing. Device-level parity catches drift early. Fallback: Tunefish-mode envelopes compute in
  closed form per 512 equivalent.
- [Random-law change audibly alters noisy WaveSabre patches (Slaughter noise, Falcon phases)] → The distribution stays
  uniform ±1. Parity metrics are spectral and loudness-based; differences show in the device report.
- [Per-voice cost of 3 slots + 4 filters + matrix makes simple songs slower than WaveSabre] → Types are dispatched once
  per control block, and off modules cost nothing. The speed bars are in the spec; profile before optimising.
- [The 64-voice cap cuts WaveSabre songs that rely on 256 voices] → The converter warns with the measured overlap.
  The cap is a constant to raise if a reference song needs it.
- [Converted automation grows the song (inserted points)] → Only where the curve error exceeds one byte step; the
  report shows sizes.
- [Param layout or chunk mistakes become a format contract] → doubt-driven-review of D3 (done: cross-model, 12
  findings, 10 fixed in D3 and the specs, 2 accepted below); version byte in the chunk.
- [A looped one-shot sample keeps its voice] → accepted: only reachable by a hand-written patch, the stealing policy
  and all-notes-off release it, and the converter never emits it.
- [The mod target table is not fixed until task 2.4] → accepted: no song or converter output exists before 6.1, and the
  table is frozen there.
- [Two copies of each algorithm during the transition] → The legacy crates are frozen, and the duplication is removed
  by `retire-legacy-synths`.
- [The `.tfm` decoder lives in `tfsynth`, which retires] → The converter depends on it now; `retire-legacy-synths`
  moves the decoder into `synth-convert` if `.tfm` import is still wanted.

## Migration Plan

1. Land `crates/gsm` (no behaviour change in `wavesabre`; its goldens must stay identical).
2. Land `unisynth` modules behind features with unit tests; legacy songs keep playing through the legacy kinds.
3. Land the converter and parity report; mixed songs can load `Either<UDevice, Either<WsDevice, TfDevice>>`.
4. When the report passes and the user signs off the A/B, open `retire-legacy-synths`. Rollback before that step is
   deleting the new crates; nothing depends on them.

## Open Questions

- Whether `add-audio-to-synth`'s `engines.toml` drops the legacy engines right away or after retirement. That change
  decides when it starts; this design does not depend on it.
