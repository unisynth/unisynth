# Tasks

## 1. Render crate

- [ ] 1.1 Create `crates/synth-render` (std, added to workspace `members` only). Features `engine-wavesabre`, `engine-tunefish`, default none. Verify `cargo build -p synth-render` and `cargo test -p synth-render` pass
- [ ] 1.2 Add the built-in `reference` engine (saw → lowpass → ADSR, 7 normalized parameters) behind the `Device` trait. Verify: unit test that parameter 0..1 maps to monotone cutoff and that the same request renders byte-identically twice
- [ ] 1.3 Implement `synth-render note` (wav or raw f32, rejects out-of-range or NaN parameters naming the index). Verify the bad-parameter and deterministic scenarios
- [ ] 1.4 Implement `synth-render batch` (NDJSON in, length-prefixed f32 out, fresh state per request). Verify order independence, and measure ≥ 200 one-second renders/s on the reference engine; record the number in the task
- [ ] 1.5 Implement `synth-render song` for `synth-mix` blobs with peak/RMS/clip report. Verify with a hand-built blob and the clipping scenario; gate on `add-wavesabre-rs` mixer being done
- [ ] 1.6 Wire `engine-wavesabre` and `engine-tunefish` as those changes land. Verify `--engine` lists compiled engines and the absent-engine scenario

## 2. Python tool scaffold

- [ ] 2.1 Create `tools/audio2synth/` with uv `pyproject`/`requirements.txt` (demucs, basic-pitch, librosa, scipy, numpy, cma, soundfile, matplotlib), `AGENTS.md`, `.gitignore` for `work/` and venvs. Verify a fresh venv installs and `python -m audio2synth --help` lists the stages
- [ ] 2.2 Add synthetic fixture generators (click track, monophonic bass line, 5-note lead, seeded) used by all tests. Verify they are deterministic and no audio is committed
- [ ] 2.3 Add JSON schema + loader/validator for `note-map/1`, `patch-fit/1`, `compare/1` with the major-version check. Verify the unknown-major scenario

## 3. Separate and transcribe

- [ ] 3.1 Implement `separate` (ffmpeg decode, demucs, sha256-keyed cache, optional `music-map/1`). Verify the cache-hit and missing-dependency scenarios
- [ ] 3.2 Implement bass transcription (pYIN, monophonic, octave check against the stem's spectral peak). Verify the monophonic-bass and octave-error scenarios on synthetic input
- [ ] 3.3 Implement lead/chords transcription (basic-pitch) and the quality gate (min length, confidence, merge). Verify the lead-melody and silent-stem scenarios
- [ ] 3.4 Write `note-map/1` with beats, bars and the opt-in `--quantize`. Verify raw times are preserved by default
- [ ] 3.5 Real-track check (local file, not committed): transcribe one user mp3, render the notes with the reference engine, listen, and record issues. Verify onset/pitch against a known-score track if one is available

## 4. Engine descriptors and loss

- [ ] 4.1 Add `engines.toml` with `reference`, then WaveSabre devices and Tunefish kind 32 as they land (names, groups, `fit` flags, default). Verify the locked-parameter scenario
- [ ] 4.2 Implement the loss (multi-res log-STFT, 5 ms envelope, centroid trajectory, RMS match). Verify identity (< 1e-6) and phase invariance (< 5%)
- [ ] 4.3 Implement reference-note selection (spread across pitch and loudness, cut with pre-roll and tail). Verify the reference-spread scenario

## 5. Fit

- [ ] 5.1 Implement the seeded CMA-ES search via `synth-render batch`, early stop, `patch-fit/1` output. Verify recovery on the reference engine (loss < 0.05 of the start) and reproducibility
- [ ] 5.2 Add staged parameter groups and measure staged versus joint on the reference engine, then on the first real device. Choose the default from the numbers; record them in `design.md` D4
- [ ] 5.3 Fit a real bass stem and a lead stem on WaveSabre (gate on `add-wavesabre-rs`) and on Tunefish (gate on `add-tf-synth`). Verify the poor-fit flag fires on an unreachable target and the loss falls for a reachable one
- [ ] 5.4 If the Tunefish fit stalls, add the spectral-envelope initialiser from the open question; otherwise record "not needed"

## 6. Assemble and compare

- [ ] 6.1 Implement `assemble` (`note-map/1` + `patch-fit/1` → `synth-mix` blob, octave fold-in with report). Verify round-trip and out-of-range scenarios
- [ ] 6.2 Implement `compare` (onset alignment, band loudness, centroid correlation, onset F1, pitch accuracy, PNG). Verify the self-comparison scenario
- [ ] 6.3 End-to-end on one real local track for each engine: `separate → transcribe → fit → assemble → compare`. Record metrics and a listening note per engine in the change; nothing from the track is committed

## 7. Docs and wrap-up

- [ ] 7.1 Add `tools/audio2synth/AGENTS.md` per-file rows, `crates/synth-render/AGENTS.md`, rows in `crates/AGENTS.md` and `tools/AGENTS.md`, and a short `docs/audio2synth.md` with the pipeline and the legal handling in D9. Verify `kb_search "audio2synth"` returns them
- [ ] 7.2 Add a project skill `audio2synth` (when to use, the five commands, how to read `compare/1`, pitfalls). Verify it lists in the skills index
- [ ] 7.3 Run `cargo test --workspace` and `bun test`, and `openspec validate add-audio-to-synth`. Verify all pass
