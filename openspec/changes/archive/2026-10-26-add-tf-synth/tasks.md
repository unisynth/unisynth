# Tasks

## 1. Crate scaffold

- [x] 1.0 Prerequisite: `add-wavesabre-rs` sections 1–2 are done (`synth-core` primitives, `Device`, voice pool, `Pool`, mixer). Verify `cargo test -p synth-core` passes
- [x] 1.1 Create `crates/tfsynth` (`#![cfg_attr(not(test), no_std)]`, `synth-core` + `libm` dependencies, features `tf-fx-<kind>` + default `tf-fx-all`). Add it to workspace `default-members`. Verify `cargo test -p tfsynth`, the i686 core-only check (`-Zbuild-std=core`) and the `wasm32-unknown-unknown` check all pass on the empty crate
- [x] 1.2 Add `params` (112 indices, public names, mapping functions per reference spec §5), and `rng` (fixed-seed xorshift32 implementing `synth-core`'s `Rng`); memory comes from `synth-core`'s `Pool`. Verify unit tests: mapping spot values (GenOct 0.5 → ×1, GenUnis 0 → 1 layer, GenPoly → voice count), RNG sequence stable
- [x] 1.3 Add the `crates/AGENTS.md` row and `crates/tfsynth/AGENTS.md` per-file table. Verify `kb_search "tfsynth"` returns the rows

## 2. Generator and voices

- [x] 2.1 Implement the `tables` init (sine, exp profile, noise) and the 512-point radix-2 complex IFFT. Verify the IFFT of a single bin gives a pure sinusoid of that bin's frequency (max error < 1e-4)
- [x] 2.2 Implement the spectrum builder (partial positions `1+s(2h−1)`, width `0.3+bw·h` cut at 5w, gain `h^−(1+damp)`, band limit), the phase stage with the reference range quirk, and table refresh on change. Verify that at GenScale 0.25 the FFT of the rendered table peaks at even bins, and that unchanged inputs don't rebuild
- [x] 2.3 Implement voice playback: unison layers, detune, spread, drive clip, octave, fixed frequency, slop, glide, pan, velocity gain, and the right-channel start phase wrapped into [0, 1). Verify tests: rendered pitch matches the note, an octave shift doubles it, the first sample is finite at maximum offset
- [x] 2.4 Implement instrument note handling on `synth-core`'s voice pool (policy `Oldest`): allocation, oldest-voice stealing, `GenPoly` limit, note-off of stolen notes, linear-Hz pitch bend on all voices, mod wheel. Verify the stealing and note-off scenarios from the `tf-synth` spec
- [x] 2.5 Add a determinism test (same patch and notes render bit-identical twice) and the `examples/render.rs` stub that renders a note to WAV. Verify the test passes and the WAV plays

## 3. Modulation, noise, filters

- [x] 3.1 Implement ADSR (block-rate, reference curves, decay-can-rise quirk) and LFOs (shapes, rates, noise steps). Verify tests: attack/decay/release reach targets within the expected block counts; each LFO shape matches its formula
- [x] 3.2 Implement the 8-live-of-10 multiplicative mod matrix with one block of slot-to-slot delay, and the unrouted-envelope organ gate. Verify that slot 9/10 routing is inert, that product composition is correct, and the gate scenario
- [x] 3.3 Implement noise (uniform, band-pass) and the LP ladder, HP/BP (`synth-core` biquad core + our log₁₀2 bandwidth designer) and notch (one-sample latency) filters with per-block coefficients. Verify the magnitude response at the cutoff and one octave either side for each type, and that filters off means bypass
- [x] 3.4 Self-golden: add in-repo synthetic patches covering sections 2–3, store FNV hashes in `tests/golden.txt` (`UPDATE_GOLDEN=1` rewrites them). Verify the test passes and that changing a mapping constant fails it

## 4. Effects

- [x] 4.1 Implement the slot chain (10 slots, `round(10p)` types, 8–10 none, shared params, state from the `Pool`, auto-sleep after 2 s below 1e-5) and the per-kind features. Verify tests: an unknown or disabled kind passes the signal through, and the auto-sleep scenario holds
- [x] 4.2 Implement distortion and delay (independent L/R, zero-latency copy quirk). Verify the distortion transfer curve at sample points and the delay impulse positions and gains
- [x] 4.3 Implement reverb on `synth-core`'s Freeverb comb/allpass (Freeverb topology, stock lengths, identical combs, right allpasses +23). Verify impulse-response energy appears at the expected comb/allpass delays per channel
- [x] 4.4 Implement chorus (flat-spot quirk, phase wrapped) and flanger (counter-flip behaviour, wrapped phase). Verify the measured sweep rate is about 0.125 Hz for LFO > 0 and the 480 s sweep of both channels in step for LFO = 0 (corrected from "static right": both channels' running term uses f), and that a 10-hour simulated run stays finite
- [x] 4.5 Implement formant (musicdsp.org vowel coefficients, f64 state, cited in source) and the 3-band EQ. Verify the formant peak frequencies per vowel and the EQ band gains at 100 Hz / 1 kHz / 10 kHz
- [x] 4.6 Extend the self-golden set with one patch per effect. Verify that building with `--no-default-features` compiles and that the effect-free goldens still pass

- [x] 4.7 Implement the `Device` adapters: instrument kind 32 (event lookahead 511, absolute block grid from `ctx.frame`) and standalone effects 33–39 with their own parameter subsets; register them in `synth-mix` behind the `tfsynth` features. Verify the "Instrument as a shared device", "Standalone effects" and "No duplicated primitives" scenarios

## 5. Song decoding and player

- [x] 5.1 Implement the `tfm` zero-copy decoder with validation (tags, I ≤ 32, array bounds). Verify that songs built in the tests round-trip, and that the malformed cases (bad tag, I = 33, truncated arrays) return errors without panicking
- [x] 5.2 Implement the player: per-instrument row cursors, block-window event firing at block start, note 0 ignored, start/seek/stop, mute with tails, song length skipping empty instruments, `required_pool`. Verify the block-quantization, empty-instrument and seek scenarios
- [x] 5.3 Implement the i16 output with the reference mix (×11, ×2000·volume, clamp) and the f32 output. Make `examples/render.rs` render a `.tfm` to WAV. Verify a synthetic song renders, and that the release build renders at ≥ 10× real time on the dev machine

## 6. Reference parity and size

- [x] 6.1 Write `tools/tf-parity/build-ref.sh`: clone upstream into `/tmp/tf-ref` and build our fresh MIT harness `render-ref.cpp` (computes song length itself). Nothing goes into the repo but the script and harness. Verify it renders `no_remorse.tfm` to `/tmp`
- [x] 6.2 Write `tools/tf-parity/parity.ts` using the shared metrics in `tools/synth-parity/` (loudness 1 dB, octave-band LTAS 3 dB, onsets ≤ 1 block; skip when the reference is absent). Verify it skips cleanly with no reference
- [x] 6.3 Run parity on `no_remorse.tfm` and `tf4testsong.tfm`. Fix mismatches (use `systematic-debugging`) until both pass, and record the metric table in `research/softsynth-research.md`
- [x] 6.4 Create `research/tf-size-probe/` (standalone workspace, no_std i686 exe rendering an in-repo synthetic song into a static buffer). Report raw and packed sizes for all features vs none via `pack.sh` in `research/softsynth-research.md`
- [x] 6.5 Update `docs/architecture.md` (audio section: tfsynth role, pool memory, quirk policy, parity tooling) and `tools/AGENTS.md`. Verify `kb_search "tf-parity"` finds the docs

## 7. Integration checks

- [x] 7.1 Run `cargo test --workspace`, `bun test` and the i686/wasm checks for `tfsynth`. Verify `tools/build-compo.sh` is unchanged and still passes (no `core::fmt`)
- [x] 7.2 A/B listen: our render vs the reference for `no_remorse` (user sign-off), noted in the change — signed off 2026-10-25: user compared `renders/tfsynth/no_remorse_{rust,upstream}.mp3` and judged them the same

## Workflow follow-up

- Run `openspec-verify-change`, then archive `add-tf-synth`. Specs `tf-synth` and `tf-song` move to `openspec/specs/`.
- Next changes: `.tfm` → `synth-mix` converter; audio output + timeline sync in the player (`required_pool` sizing from the exporter); later the merged instrument.
