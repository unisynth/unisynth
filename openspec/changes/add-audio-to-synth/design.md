# Design

## Context

Both synth engines (WaveSabre port, Tunefish-compatible) are being built on `synth-core`: the `Device` trait (normalized 0..1 parameters, `note_on/off` with sample offsets, block `run`) and the `synth-mix` song blob (device kinds as numeric ids; WaveSabre 0–12, Tunefish 32–39). They will be merged later. Analysis tooling exists in the dashboard repo (`music-analysis`: `music-map/1`, demucs stems). No transcription or timbre fitting exists.

## Goals / Non-Goals

**Goals**
- mp3 → bass + lead notes → fitted patch per stem → rendered song with A/B metrics, for both engines through one code path.
- Stages communicate through versioned JSON, so each can be rerun, inspected and tested alone.
- Reproducible runs (fixed seeds, deterministic synth).

**Non-goals**
- Drums, chords, vocals, effects fitting, learned synth models, real-time operation, a GUI.
- Committing any third-party audio or model weights.

## Decisions

### D1. Python for analysis, Rust for rendering, process boundary between them
Demucs, basic-pitch, librosa and CMA-ES are Python. The synth is `no_std` Rust. They meet at `synth-render`, which keeps the fitter engine-agnostic and means no Rust bindings (pyo3) to maintain across the two engines. Alternative: pyo3 or wasm bindings. Rejected for now: heavier build, and a persistent `batch` subprocess already removes per-call start-up.

### D2. Engine-neutral fitting via normalized parameters
Every `Device` takes 0..1 parameters, so the search space is `[0,1]^n` for any engine. `engines.toml` carries what the vector means (names, groups, `fit` flags). Adding a device (or the later merged instrument) is a descriptor edit, not new fit code. Alternative: per-engine fitters. Rejected: doubles the work and drifts.

### D3. Loss ignores phase, matches envelope and brightness over time
Synth output never lines up in phase with a recording. Multi-resolution log-STFT captures spectral shape, the 5 ms amplitude envelope captures attack/decay (which is most of what separates a pluck from a pad), and the centroid trajectory captures filter movement. Level is matched by RMS first, so the fit does not spend parameters on loudness (velocity and track volume cover it). Alternative: a learned perceptual loss. Rejected: heavy, not reproducible.

### D4. CMA-ES, bounded, seeded; staged groups
Parameters are many and interact (filter vs oscillator brightness). CMA-ES handles that without gradients. To keep dimensionality manageable the search runs in groups (osc + env first, then filter + lfo, then joint polish) when the descriptor supplies groups. Fallback is one joint run. Open: whether staging beats one joint run on Tunefish's 112 parameters; the fit task includes a measurement and the default follows the result.

### D5. Batch rendering over a persistent process
A fit does ~10⁴–10⁵ renders. `synth-render batch` reads NDJSON requests and returns length-prefixed f32 buffers, one fresh device state per request (order independent). The budget is checked in a performance task: target at least 200 note renders per second for a 1 s note on the dev machine. Alternative: spawn per render. Rejected on cost.

### D6. Note times come from audio; quantization is opt-in
Transcribers place onsets with their own error. Snapping to a grid hides this and breaks swing and live-played parts. The file keeps raw times plus beats for later quantization, and `assemble` can quantize on request.

### D7. Stems feed fitting but the full mix is the final judge
Demucs output has artefacts (bleed, smeared transients). Patches are fitted against stem notes, but `compare` also reports metrics on the stem and, when the original is available, on a mix of rendered parts versus the original mix, so a good fit on a bad stem is visible.

### D8. Built-in `reference` engine for early development
A tiny engine (saw, lowpass, ADSR) inside `synth-render` lets the whole pipeline be built and tested before the two real engines land, and gives the fit its known-answer recovery test (D3 scenario).

### D9. Legal and data handling
Inputs are local user files. Demucs and basic-pitch are installed in a venv, not vendored, and weights are downloaded on demand to a cache outside the repo. Test fixtures are synthetic (generated at test time). Any song-derived output (notes, patches, wavs) goes under an untracked `work/` dir.

## Data contracts

| Contract | Producer → consumer | Key fields |
|---|---|---|
| `music-map/1` (existing) | analysis → separate/transcribe | tempo, bars, key |
| `note-map/1` | transcribe → assemble, compare | tracks[].notes[] {pitch, start_s, dur_s, start_beat, bar, velocity, confidence} |
| `patch-fit/1` | fit → assemble | engine, device_kind, params[], loss, seed |
| `compare/1` | compare → human | band loudness, centroid corr, onset F1, pitch acc |
| `synth-mix` blob (existing) | assemble → synth-render song | tracks, devices, events |

## Risks / Trade-offs

- **Wrong notes poison the fit.** Mitigation: the note stage ships and is checked first (spec scenarios with synthetic input), the fit uses reference notes with the highest confidence.
- **Engine can't make the sound** (a sample-based pad, vocal). The loss plateaus. Mitigation: the report shows the final loss against a threshold and flags "poor fit", and does not pretend.
- **Local minima.** Mitigation: multiple seeds, staged groups, a patch-bank warm start (render the engine's presets, start from the best). The bank is a follow-up if needed.
- **Depends on unfinished engines.** Mitigation: D8, and tasks gate engine-specific tests on the engine change being done.
- **Fit time on a laptop.** Mitigation: D5 budget, fewer reference notes, shorter notes.
- **Copyright.** D9. Nothing derived from a user's track is committed.

## Migration Plan

Additive, nothing shipped changes. Rollback is deleting `tools/audio2synth/` and `crates/synth-render`.

## Open Questions

- Staged versus joint search (D4), decided by measurement on the reference engine and then on the first real device.
- Whether Tunefish's wavetable generator needs a spectral-envelope initialiser (fit partial amplitudes directly from the target, then refine) rather than blind search. Decide after the first Tunefish fit result.
- Where the pyproject lives (tools/audio2synth versus packages/). Default: `tools/audio2synth/`.
