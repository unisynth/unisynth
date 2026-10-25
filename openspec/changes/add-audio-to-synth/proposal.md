# Proposal

## Why

We want to take an existing track (mp3), pull out its bass, lead/theme and drum parts, and recreate them with our own soft synth so they sound close to the original. Doing this by hand means listening, guessing notes and tweaking dozens of knobs per instrument. The pieces exist, but nothing connects them:

- `music-analysis` (pi-dashboard package `music-production`) gives tempo, bars, key, sections and demucs stems, but no notes and no timbre.
- The synth is being built as two engines on one shared core: WaveSabre (`add-wavesabre-rs`) and a Tunefish-compatible engine (`add-tf-synth`), to be merged later. Both expose normalized 0..1 parameters behind `synth-core`'s `Device` trait and render through the `synth-mix` song blob.

A closed analysis-by-synthesis loop closes the gap: transcribe notes per stem, fit a patch per instrument by rendering candidate patches and comparing them with the original audio, then write a song and render it for an A/B check. Because both engines share `Device` + normalized parameters + `synth-mix`, the fitter is engine-neutral and one loop serves both.

## What Changes

- New Python tool `tools/audio2synth/` (uv venv, separate from the Bun and pnpm toolchains; the Python deps are `demucs`, `basic-pitch`, `librosa`, `scipy`, `cma`). Pipeline stages, each a CLI that reads and writes versioned JSON:
  1. **separate**: mp3 → decode via ffmpeg → stems (bass, drums, vocals, other). Reuses the `music-map/1` bar grid when one exists, otherwise runs the quick librosa tier.
  2. **transcribe**: per stem → `note-map/1` JSON (notes with start/length in beats and seconds, MIDI pitch, velocity, bar index). Bass uses monophonic pYIN, lead and chords use basic-pitch, drums use onset detection plus band-energy classification (kick, snare, hat).
  3. **fit**: per stem, pick representative single notes, then search patch parameters (CMA-ES) by rendering through the synth and minimising a multi-resolution STFT + envelope loss. Output: `patch-fit/1` JSON (engine, device kind, normalized parameter vector, loss, per-note-class variants).
  4. **assemble**: `note-map/1` + `patch-fit/1` → a `synth-mix` song blob (one track per stem, one device per track), written for WaveSabre devices or Tunefish kind 32.
  5. **compare**: render the song, align with the original stem, report spectrogram diff, loudness, onset and pitch agreement as `compare/1` JSON plus a PNG.
- New std Rust crate `crates/synth-render` with one binary:
  - `synth-render note` renders one note with a given engine/device/parameter vector to WAV or raw f32 (used by the fitter, many thousands of calls, so it can also serve a batch over stdin).
  - `synth-render song` renders a `synth-mix` blob to WAV.
  - Engines are cargo features (`engine-wavesabre`, `engine-tunefish`), so the crate builds with either engine present.
- Engine neutrality is a requirement: no engine-specific code in the Python tool except a small table (`engines.toml`) that names each device, its parameter count, fit-relevant parameters and parameter groups.
- Fixed seeds everywhere, so a run is reproducible from the same input and the same synth build.

Scope for the first version (user decision): one track, **bass and lead melody**. Drums and chords are specified in `note-map/1` but the stages for them are out of scope here; patch fitting starts after the note stage produces correct notes.

Out of scope:
- Drum and chord reproduction (schema reserves them).
- Vocals, lyrics, and any generative or learned synth model.
- Effects fitting (reverb, delay, EQ on the stem). The first version fits the instrument patch only; the effect chain is a follow-up.
- Sourcing audio: the user supplies a local file. No stream ripping, no committed third-party audio. The output of a copyrighted track is for study and tests locally; nothing derived from it is committed.
- Real-time use, GUI, and the editor integration.

## Discipline Skills

`performance-optimization` (the fit loop renders thousands of notes; batch rendering and a cheap loss are the budget), `doubt-driven-review` (the `note-map/1` and `patch-fit/1` formats are contracts between stages and engines), `systematic-debugging` (transcription and fit mismatches).

## Capabilities

### New Capabilities
- `audio-transcribe`: stem separation, note extraction and the `note-map/1` contract.
- `synth-patch-fit`: engine-neutral patch search, loss definition, `patch-fit/1` and the assemble and compare stages.
- `synth-render`: the `synth-render` crate and CLI used by the fitter and for A/B renders.

### Modified Capabilities
- (none)

## Impact

- Depends on `add-wavesabre-rs` (`synth-core`, `synth-mix`, WaveSabre devices) and `add-tf-synth` (Tunefish device). The pipeline and the transcription stages can be built and tested earlier against a built-in reference sine/saw device in `synth-render`. Fit and assemble tests for each engine become runnable as that engine lands.
- New `tools/audio2synth/` (Python, own `requirements.txt` + `AGENTS.md`), new `crates/synth-render` (added to workspace `members`, not `default-members`, not linked into the player).
- `crates/AGENTS.md`, `tools/AGENTS.md`, `docs/` pointer and a project skill `audio2synth` describing how to run the pipeline.
- No change to the player, existing crates or specs.
- Licensing: demucs (MIT) and basic-pitch (Apache-2.0) run as external dev tools in a venv, not vendored. No audio or model weights are committed.
