# Proposal

## Why

add-v2-import put Farbrausch V2 next to the unisynth modules as separate code: filter kinds `V2_*`, envelope mode V2,
LFO mode V2, oscillator type V2 and a `v2.voice` switch that changes the voice's output stage. Read side by side, most
of it is the same algorithm as a unisynth module with different settings: the WaveSabre SVF and the V2 LRC filter are
the same state-variable filter; the ladder and the V2 Moog are the same 4-pole filter; the three envelope modes share
a linear attack and differ in decay/release curve, time law and sustain; the two LFO modes share the phase
accumulator. Keeping them apart doubles code a song can carry, and V2's options (sustain slope, amplify, polarity,
one-shot, sample-and-hold, ring) stay locked to V2 patches. The user wants V2 to be settings of shared algorithms,
exposed as parameters any patch can use, and named by what they do: the
synth itself no longer names V2, Tunefish or WaveSabre.

## What Changes

- **Filters.** One state-variable filter and one 4-pole filter, each a single formula whose coefficients the settings
  choose. Their differences become parameters:
  response (low/band/high/notch/all-pass), resonance form, 2× oversampling form, feedback clip, cutoff/resonance law.
  RBJ biquads and the resonator notch stay their own topologies.
- **LFOs.** One LFO: the union of shapes (sine, ramp up/down, square/pulse, triangle, stepped random, sample-and-hold),
  and polarity, one-shot, start phase and rate law as parameters of every LFO.
- **Envelopes.** One envelope engine: linear attack; decay and release curve (quadratic, power-slope, exponential);
  sustain slope and amplify; time law (timed, rate, V2). Every envelope can combine them.
- **Oscillators and voice.** The combine operation into the running sum (add, ring/multiply, replace) becomes a
  parameter of every oscillator slot. V2's waveform generators (naive saw/triangle morph, pulse, sine, coloured noise,
  FM from the running sum, aux input) stay generator kinds. `v2.voice` is replaced by voice parameters (amplitude
  source, pan stage, output DC filter); no separate V2 voice path remains.
- **Functional names.** Parameters, option values, cargo features, module ids, effect modes and doc sections are named
  by function (for example `us-osc-naive`, `drive.*`, `strip.*`, envelope laws `seconds`/`rate`/`frame`, reverb
  `comb`, compressor `lookahead`). Source names stay only where the subject is the source: converter commands
  (`ws`, `tfm`, `v2m`), parity against a reference, and provenance tables.
- **Era behaviours ("quirks").** Behaviour that exists only to match particular V2 builds (fast vs native sine and
  arctangent, `fistp` FM collapse, MSVC LCG noise, 4× box-filtered oscillators, 256-frame control grid, year-2000
  envelope laws, DC bias, Moog bypass before format 6, …) becomes named quirk flags. Each is documented with the format
  versions and the corpus songs that need it, and is a feature only those songs build in. A quirk is kept only if a
  song falls outside the parity tolerance without it; the others are deleted.
- **BREAKING (format).** The parameter layout and the chunk version change; the player reads only the new version.
  The converters (`ws`, `tfm`, `v2m`) write the new parameters; a host-side migration converts chunks and song files
  written in the old layout (DAW-authored songs and patches), so nothing has to be re-authored. The old mode/kind
  choices remain as host-side presets that set the new parameters.
- **Fidelity and cost gates per stage.** Sound within tolerance, not bit parity: every parity report (the WaveSabre
  and Tunefish device items and six reference songs, the 15 V2 acceptance songs and the 3 aux songs) still passes its
  tolerance (1 dB loudness, 3 dB per octave band, onsets). The drift against the previous build is recorded per song.
  Self-goldens are regenerated per stage after the parity gate passes. Reference songs do not grow in packed size;
  every stage records its size and speed.

## Capabilities

### New Capabilities
<!-- none -->

### Modified Capabilities
- `unified-synth`: filter slots, envelopes, LFOs, oscillator slots, the V2 voice path, V2 module features and the chunk
  format become shared algorithms with exposed settings, functional names and documented quirk flags.
- `synth-convert`: the converters write the new settings, and a migration converts old-layout chunks and song files;
  the feature list names quirk flags; the converters map source names to functional ones.
- `unified-fx`: the V2 effect modes get functional names (comb reverb, modulated delay, look-ahead compressor).

## Discipline Skills

`performance-optimization` (size and speed measured every stage), `doubt-driven-review` (the parameter layout, names and
chunk version before they stand), `systematic-debugging` (a parity item that leaves tolerance), `code-simplification`
(after each merge).

## Impact

- `crates/unisynth`: `flt.rs`, `v2.rs`, `env.rs`, `lfo.rs`, `osc.rs`, `voice.rs`, `params.rs`, `chunk.rs`, `needs.rs`,
  `module.rs`, `instrument.rs`, `Cargo.toml` features; tests and self-golden.
- `crates/synth-convert` (`ws`, `tf`, `v2conv`, `features`), `crates/song` (tables, import/migration, schema),
  `crates/synth-wasm` and the DAW device panel (parameter tables; coordinate with the active `add-synth-daw` change).
- Songs in `project/music/` and `research/us-size-probe/out/songs/` are migrated or re-converted.
- Docs: `docs/synth/unisynth.md`, `research/v2-synth.md`; reports in `tools/unisynth-parity/`.
