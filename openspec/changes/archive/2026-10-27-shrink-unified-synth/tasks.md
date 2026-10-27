# Tasks

## 1. Measure

- [x] 1.1 `research/us-size-probe/marginal.sh`: all-modules build against each leaf feature removed (raw, xz) → `out/marginal.csv`. Verify: table in design.md.
- [x] 1.2 `research/us-size-probe/attr.sh` + `attr.py`: size per function group from the linker map (no LTO, no inlining). Verify: libm, mix, voice shares in design.md.
- [x] 1.3 `research/size-compare`: legacy against unified for the six songs under six compile configurations (`tools/unisynth-parity/size-compare.md`).

## 2. Module code leaves with its feature

- [x] 2.1 Guard the oscillator (PM, saw/pulse, sampler), noise and filter (SVF, ladder, RBJ, notch) arms with `cfg!(feature)`; gate the tests that call them directly. Verify: `tools/check-unisynth-features.sh` (every single-module set, i686 + wasm32 + tests, no warnings), `tools/check-song-features.sh --minimal` on Global Groove, no_remorse and Elysian, `size-compare` re-run: every song smaller, goldens unchanged.
- [x] 2.2 Spec: requirement "Module code is removed with its feature".

## 3. The map says why and what it costs

- [x] 3.1 `synth-convert features --explain` / `--json`: per feature, what requires it (device, parameter, lane) and its marginal cost when `out/marginal.csv` exists. Tests: a song with one noise instrument names the instrument and the level parameter; a lane-reached filter type names the lane. Verify: the output for the six reference songs reads right.
- [x] 3.2 Docs: `docs/synth/unisynth.md` "Module features" section: the map, `--explain`, the cost table, `attr.sh`.

## 4. Candidates (each lands only if a reference song packs smaller by more than 512 B and parity, feature gate and goldens hold)

- [x] 4.1 `libm`: bounded-argument `pow`/`exp2`/`log10`/`sin`/`cos` in `unisynth::fm` (not `synth-core::math`: the legacy engines share that and their goldens are bit-exact with upstream); remove `rem_pio2`. Done: `libm` 11.8 KB → 0.3 KB, −3 to −4.5 KB packed per song; effect parity tests that were bit-exact now allow 1e-5 (crusher 1e-4), self-goldens regenerated, full parity 49/49. Measure packed size of every song and the legacy engines; run the full parity; regenerate self-goldens only with the parity result in the same commit.
- [x] 4.2 Voice switches: ablation of unison, mono/glide/slide, LFO, controllers, one-shot, start-spread, fine grid, stealing; add a feature for each that saves ≥ 0.5 K raw, with the needs analysis and the feature gate. Done: `research/us-size-probe/ablate.sh`; features `us-voice-{lfo,glide,mono,unison,oneshot}` (load check `check_voice`, mapping in `needs.rs`, `VoicePool` gated by `Voice::MONO`/`UNISON`); controllers, start-spread, fine grid, stealing < 0.5 K, kept. Packed −1 KB Global Groove and tf4testsong, −0.5 KB engage, no_remorse, turtles, 0 Elysian/Eladamri; goldens unchanged, reduced builds play every loadable golden patch identically.
- [x] 4.3 Matrix target groups: ablation, then features if ≥ 1 K. Done: per-group gates grow the player (+0–0.5 K raw), every unused target together saves 0.5 K raw, so no group feature; the whole matrix is 2–2.5 K raw: `us-matrix` (`modmx::routes`, `check_voice`, `needs.rs`), Global Groove −1 KB packed (15,872 → 14,848), the other reference songs route and keep it.
- [x] 4.4 Decide on baked patches (`doubt-driven-review` first): from the size left after 4.1–4.3, record the decision and the number in design.md. Done, step 1 (two cross-model review cycles): `synth-convert rust` writes the song as Rust (validated on the host with the real player, the blob's own section bytes, compile-time module assert); `unisynth/us-trusted` drops chunk validation and the module load checks, `Song::parse` is not linked. −512 to −1,024 B packed on every reference song (Global Groove 13,824, under legacy); `research/us-bake-check/check.sh` bit-identical on all seven.
- [x] 4.5 Step 2 of 4.4, voice specialization from generated `const` facts (used slot kinds, routes, unison): measure and decide (`doubt-driven-review` first); per-instrument copies of the voice may grow the exe. Done: per-instrument specialization not pursued (one ~9.5 K voice copy per instrument); song-wide facts checked: slot counts and sample rate do not fold; no automation lanes does: `us-automation` (`Device::automation()`, refused at load without it), −512 B packed on the four songs without lanes (exactly the squishy step, accepted by the user), byte-identical exe for songs with lanes, bake check bit-exact.

## 5. Close

- [x] 5.1 Re-run `size-compare`, update `research/softsynth-research.md` and the parity report size table; `openspec validate --strict`.
