# Proposal

## Why

The intro needs music, and it needs a synth that musicians can already compose with on our Windows and macOS machines today. Tunefish 4 meets that: a cross-platform VST/AU, a ~10 KB code core, and songs of 1–2 KB compressed. It is GPL-3, while the intro and our crates ship under MIT. We therefore write our own MIT `no_std` Rust synth that sounds like Tunefish 4 and plays its patches and `.tfm` songs. We understand the algorithms from the original source and the measured behavioural spec (`research/tunefish4-functional-spec.md`), but copy no code, comments, tables or identifiers.

## What Changes

- **Depends on `add-wavesabre-rs`**, which adds `crates/synth-core` (shared DSP primitives, `Device` trait, voice pool, pool memory, `synth-mix` song/mixer). This change uses those and keeps no private copies.
- New `no_std`, f32 Rust crate `crates/tfsynth` (MIT) with no heap and no `core::fmt`, at 44 100 Hz and 512-sample blocks:
  - Per voice: a spectral IFFT wavetable generator (harmonics, bandwidth, damp, scale, unison, detune, spread, drive, octave, glide, slop), stereo noise with band-pass, LP/HP/BP/notch filters, 2 ADSR, 2 LFO, and an 8-slot multiplicative mod matrix.
  - 16-voice allocation with oldest-voice stealing, per-patch polyphony, pitch bend and mod wheel.
  - Up to 10 serial insert effects per instrument: distortion, delay, reverb, chorus, flanger, formant, EQ. Each effect kind is a cargo feature (`tf-fx-<kind>`), matching the per-kind feature convention of the engine.
  - The 112-parameter instrument layout, so existing Tunefish 4 patches (normalized 0..1 values) load unchanged.
- Combination with other engines (user decision 2026-10-25, options 1 + 2):
  - The Tunefish instrument implements `synth-core`'s `Device` (kind id 32), so it can sit on any `synth-mix` track next to WaveSabre devices. Its 10 internal effect slots stay inside it.
  - Each of the 7 effects is also a standalone `Device` (kind ids 33–39) with its own parameter set, usable on any track.
- `.tfm` song support: zero-copy decoding of the song bytes (instrument patches + per-instrument event lists), event scheduling at 4 rows per beat, and rendering to an interleaved stereo i16 / f32 buffer.
- Quirk policy: quirks that shape the sound are reproduced (phase-stage table range, 8 live mod slots, block-rate envelopes/LFOs, flanger counter flip, effect auto-sleep, HP/BP log₁₀2 bandwidth, linear-Hz bend, `.tfm` 1/100 quantization). Crashes and undefined behaviour are fixed: empty-instrument song length, right-channel phase overrun, unwrapped chorus phase, unbounded flanger counter, uninitialized voice state. Randomness uses a fixed seed, so renders are bit-for-bit reproducible.
- Verification tooling:
  - An `examples/render.rs` (std) `.tfm` → WAV renderer.
  - An opt-in parity tool that compares our render against a reference render of the original. Loudness, spectrum and onset metrics must stay within tolerances.
  - The reference renderer is built outside the repo from upstream source by a script. **No GPL code, reference WAVs or third-party songs are committed.**
- Size report for the compo target (i686, `no_std`, all features vs. a minimal feature set), packed with the existing packer harness.

Out of scope: audio device output and timeline sync in the player (next change), the editor/wasm preview UI, converting `.tfm` songs into the `synth-mix` format (a later change), the merged instrument combining both engines' voices (a later change), 64klang, GM.DLS playback (`crates/dls` stays separate), our own plugin/UI (musicians keep using the original Tunefish 4 plugin to author songs).

## Discipline Skills

`performance-optimization` (render must run well above realtime for prerender, plus the compo code-size budget), `doubt-driven-review` (the `.tfm` format and parameter layout are a compatibility contract that is hard to change later), `systematic-debugging` (parity mismatches against the reference render).

## Capabilities

### New Capabilities
- `tf-synth`: Tunefish-4-compatible instrument engine. Covers the parameter layout and mappings, voice lifecycle, generator, noise, filters, envelopes, LFOs, mod matrix, effects chain, determinism and the quirk policy.
- `tf-song`: `.tfm` song decoding and playback. Covers byte layout, event scheduling, song length, full-song render API, reference parity verification and the size report.

### Modified Capabilities
- (none)

## Impact

- New crate `crates/tfsynth`, added to workspace `default-members`. It depends on `synth-core` and `libm`.
- `synth-mix` device registry gains kind ids 32–39 (behind `tfsynth` features).
- New `tools/tf-parity/` (shell + Rust std example): fetches and builds the upstream reference renderer under `/tmp` and compares WAVs with the shared metrics in `tools/synth-parity/` (from `add-wavesabre-rs`). It is never part of the player build.
- `crates/AGENTS.md`, `docs/architecture.md` (audio section), `research/softsynth-research.md` pointers.
- No change to existing specs or crates. The player integration comes later.
- Licensing: our code stays MIT. The reference renderer is GPL-3 and runs only as an external, uncommitted dev tool.
