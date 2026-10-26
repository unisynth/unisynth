# Proposal

## Why

The unified synth plays all six reference songs within parity, but packs larger than the legacy engines
(`tools/unisynth-parity/size-compare.md`, squishy 0.1.3, compo configuration, legacy → unified): Elysian 43.5 → 48.6 KB
(+12 %), Engage 36.4 → 41.5 (+14 %), Eladamri 30.2 → 34.3 (+14 %), no_remorse 17.9 → 28.2 (+57 %), tf4testsong 17.4 → 26.6
(+53 %), Global Groove 14.3 → 20.0 (+39 %). The songs themselves pack 0–2 KB larger; the rest is engine code. Compile
options change the absolute size by 3–30 % but not the ratio. The song knows exactly what it uses, so the build can carry
less: the module map (`synth-convert features`) already does that per module; this change goes finer, measure first.

## What Changes

- **Measured first.** Two repeatable tools say where the bytes are: `research/us-size-probe/attr.sh` (size per function
  group, no LTO and no inlining so every function keeps a symbol) and `marginal.sh` (all modules against each one
  removed). Every step below must lower the packed size of a reference song, or it is dropped.
- **Landed (step 1).** Oscillator, noise and filter code was reachable in every build: the feature only made the load
  fail. Match arms are now guarded by `cfg!(feature)`, so the compiler drops the code of a module the build lacks.
  Measured: −1 to −2 KB packed on every reference song (−2 KB on Global Groove, −1.5 KB on no_remorse, xz).
- **Song map with reasons and cost.** `synth-convert features --explain` (and `--json`) lists each feature with what
  requires it (device, parameter, automation lane) and its measured marginal cost, so the map tells what a song pays for.
- **Finer switches** where the numbers say they pay: voice paths (unison, mono/glide, LFO, controllers, one-shot), mod
  matrix sources and targets, and replacing the generic `libm` functions with range-limited ones (both legacy engines
  carry the same ~10 KB of `libm`; unified carries 12).
- **Decision point.** Baking the song's constant parameters into the build (a generated `Spec` per song) only if the
  numbers above leave a gap worth that refactor; the DAW keeps the runtime path.

## Capabilities

### Modified Capabilities
- `unified-synth`: module features SHALL remove the module's code, not only reject its patches; the needs map is explained.

## Discipline Skills

`performance-optimization` (measure before every change, keep the number), `doubt-driven-review` (before the build-time
specialisation, which changes the build model), `code-simplification` (if a switch costs more code than it saves).

## Impact

- `crates/unisynth` (`flt.rs`, `osc.rs`, `voice.rs`, `noise.rs`, later `modmx.rs`), `crates/synth-core/src/math.rs`,
  `crates/synth-convert` (`features --explain`), `research/us-size-probe`, `research/size-compare`.
- No change to the song format, the chunk or the parameter layout. Output stays identical for builds that have every
  module (self-goldens unchanged); the `libm` step may move goldens and must pass parity.
