# Tasks

## 1. synth-core scaffold and primitives

- [x] 1.1 Create `crates/synth-core` (`#![cfg_attr(not(test), no_std)]`, `libm`) and add it to workspace `default-members`. Verify `cargo test -p synth-core` passes, and the i686 core-only (`-Zbuild-std=core`) and `wasm32-unknown-unknown` checks pass
- [x] 1.2 Implement `math`, `pool` (bump allocator, alignment, `OutOfMemory`), `rng` trait and `denormal` guard (x86 MXCSR, AArch64 FPCR, restore on drop). Verify unit tests: note→Hz at 69/81, pan law, pool alignment/overflow, float mode restored
- [x] 1.3 Implement `dsp/{comb,allpass,delay,biquad,onepole}` with RBJ designers as free functions. Verify the comb impulse, biquad gain and fractional-delay scenarios of the `synth-core` spec
- [x] 1.4 Add the `crates/AGENTS.md` row and `crates/synth-core/AGENTS.md`. Verify `kb_search "synth-core"` finds them

## 2. Device framework and mixer

- [x] 2.1 Define the `Device` trait, `Io` (4 channels), `Ctx` (tempo, rate, rng) and the event buffer with sample offsets. Run `doubt-driven-review` on the trait against the Tunefish needs (512-frame internal blocks, shared FX params) before continuing. Verify a test device that records calls sees notes at the right offsets
- [x] 2.2 Implement `voice` (pool, `None`/`Oldest` stealing, `Poly`/`MonoLegato` with slide, unison detune/pan spread). Verify the no-stealing, oldest-stealing and mono-legato scenarios
- [x] 2.3 Implement `mix/song` (in-place decoder of the WaveSabre blob, all bounds checked) and `mix/registry` (enum dispatch, unsupported-kind error at load). Verify the round-trip, malformed-input (truncate at every byte, out-of-range indices; no panic) and feature-disabled scenarios
- [x] 2.4 Implement `mix/track`, `mix/automation` and `mix/player` (dependency order, receives into main/sidechain, track volume, 100-frame sub-blocks, `render_f32`/`render_i16`, `seek`, `length_frames`, `required_pool`). Verify the sidechain, dependency-order, automation, buffer-size independence and seek scenarios with test devices

## 3. WaveSabre helpers and effects

- [x] 3.1 Create `crates/wavesabre` (MIT notice from upstream, features `ws-<device>` + `ws-all`) with `helpers`: FastSin/FastCos table, parameter mappings, random law. Verify the random-law scenario and FastSin against `sin` within the table error
- [x] 3.2 Port Crusher, Scissor and Leveller. Verify the Crusher step/hold law, the Scissor transfer curves with 2×/4× oversampling, and the Leveller band gains
- [x] 3.3 Port Echo, Chamber and Smasher (delay rings from the pool, worst-case sizing, sidechain input). Verify Echo tap times at 120 BPM, the Chamber impulse response, and Smasher gain reduction above the threshold with a sidechain key
- [x] 3.4 Port Cathedral (on the shared Freeverb comb/allpass) and Twister (fractional reads). Verify the Cathedral impulse peaks at the comb lengths (+23 on R), freeze holds energy, and Twister's sweep stays within 2..134 samples

## 4. WaveSabre instruments

- [x] 4.1 Port the `SynthDevice` behaviour onto the `synth-core` voice pool and event buffer, plus the shared envelope and SVF (WaveSabre-specific modules). Verify envelope timing in ms and the SVF response
- [x] 4.2 Port Slaughter and Falcon. Verify rendered pitch, the saw/pulse spectrum shape (BLIT), the FM sidebands at known ratios, and the start-phase random draws in song order
- [x] 4.3 Implement the `gsm` decoder (WAV49, fixed-point, libgsm notice). Verify exact equality with libgsm on test vectors generated in the lab and checked in as our own encoded noise/sine blocks; malformed input returns an error
- [x] 4.4 Port Specimen and Thunder (GSM chunk layout, loop and interpolation modes) and Adultery (waves through `crates/dls`, silence without a bank). Verify sample playback pitch and loop points on an in-test sample, the no-bank scenario, and Adultery on the macOS bank when it exists (env-gated)
- [x] 4.5 Add `examples/render.rs` (song blob → WAV) and the self-golden test (`tests/golden.txt`, `UPDATE_GOLDEN=1`) over in-repo synthetic songs covering every device. Verify the goldens pass and that two renders are bit-identical

## 5. Parity, speed, size

- [x] 5.1 Create `tools/synth-parity/` (bun: loudness, octave LTAS, onset metrics + tests on synthetic signals) for use by both parity tools. Verify identical → pass, +3 dB → fail
- [x] 5.2 Create `tools/ws-parity/`: a script that builds upstream natively from the lab checkout (`wavesabre-native-macos-render` method) plus our MIT `render-device.cpp`; skips cleanly when absent. Verify per-device SNR ≥ 60 dB for every device (use `systematic-debugging` on mismatches)
- [x] 5.3 Run song parity on Global Groove, Elysian, Engage (with Windows `gm.dls`) and eladamri at 1 thread / 100-frame steps. Record the metric table in `research/softsynth-research.md`. Verify all pass
- [x] 5.4 Measure release render speed for Elysian (≥ 5× real time). Use `performance-optimization` if it misses. Record the result
- [x] 5.5 Create `research/ws-size-probe/` (no_std i686, renders an in-repo synthetic song). Report raw and packed sizes for `ws-all` vs Slaughter-only in `research/softsynth-research.md`

## 6. Integration and docs

- [x] 6.1 Update `docs/architecture.md` (audio section: synth-core layers, Device trait, mixer, engines, pool, parity tooling), `crates/AGENTS.md`, `tools/AGENTS.md`. Verify `kb_search "synth-mix"` finds them
- [x] 6.2 Run `cargo test --workspace`, `bun test`, and the i686/wasm checks for both crates. Verify `tools/build-compo.sh` still passes (no `core::fmt`)
- [x] 6.3 A/B listen: our Elysian render vs upstream (user sign-off), noted in the change — signed off 2026-10-25: user listened to `renders/wavesabre/elysian_{rust,upstream}.mp3` and judged them the same ("it seems ok")

## Workflow follow-up

- Run `openspec-verify-change`, then archive. Specs `synth-core`, `synth-mix` and `ws-synth` move to `openspec/specs/`.
- Then implement `add-tf-synth` (rebased on `synth-core`), then a later `.tfm` → `synth-mix` converter change.
