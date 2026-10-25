# Spec Delta

## Purpose

Finds synth parameters that make one instrument sound like a target stem, engine-neutrally, then builds and checks a song.

## ADDED Requirements

### Requirement: Engine descriptor
Each engine device SHALL be described in `engines.toml`: engine id, device kind id, parameter count, and per parameter a name, default, group (osc, filter, env, lfo, fx, other) and a `fit` flag. The fitter SHALL touch only parameters whose `fit` flag is set and SHALL keep all others at their default.

#### Scenario: Locked parameter
- **WHEN** a parameter has `fit = false`
- **THEN** it keeps its default value in every candidate and in the result

### Requirement: Fit target
The `fit` stage SHALL choose up to N reference notes per stem (default 6) spanning the stem's pitch range and loudness, cut them from the stem with a short pre-roll and a release tail, and use them as the target. The patch SHALL be rendered at each reference note's pitch, velocity and length.

#### Scenario: Reference spread
- **WHEN** a bass stem spans MIDI 28..52
- **THEN** the chosen reference notes include one in the lowest and one in the highest quarter of that range

### Requirement: Loss
The loss SHALL be a weighted sum of (a) multi-resolution log-magnitude STFT distance (FFT sizes 512, 2048, 8192), (b) amplitude-envelope distance in dB at 5 ms resolution, (c) spectral-centroid trajectory distance. Both signals SHALL be level-matched by RMS before comparison, and loudness SHALL be reported separately. Phase SHALL NOT enter the loss.

#### Scenario: Identity
- **WHEN** the target is rendered by the same patch
- **THEN** the loss is below 1e-6

#### Scenario: Phase invariance
- **WHEN** the target is the same note shifted by a random initial phase
- **THEN** the loss differs from the loss with no shift by less than 5%

### Requirement: Search
The search SHALL use CMA-ES over the unlocked normalized parameters, bounded to [0, 1], with a fixed seed, a population and generation budget from the command line, and early stop when the best loss improves under 1% over 10 generations. Candidates SHALL be rendered through `synth-render batch`. The result SHALL be reproducible given the seed, the synth build and the input.

#### Scenario: Recovery on the reference engine
- **WHEN** the target is a `reference`-engine note with known parameters and the fit starts from defaults
- **THEN** the final loss is below 0.05 of the starting loss within the default budget

#### Scenario: Reproducible
- **WHEN** the fit is run twice with the same seed
- **THEN** both runs give the same parameter vector

### Requirement: Patch contract
The stage SHALL write `patch-fit/1`: `{schema, engine, device_kind, params[0..1], loss, loss_terms, seed, budget, reference_notes[], synth_build}`. A consumer SHALL reject an unknown schema major version.

#### Scenario: Unknown major
- **WHEN** a consumer reads `patch-fit/2`
- **THEN** it exits with an error naming the expected version

### Requirement: Assemble
The `assemble` stage SHALL turn `note-map/1` and `patch-fit/1` into a `synth-mix` song blob: one track per stem with one instrument device, note events at sample-accurate offsets from `start_s`, tempo from the map, and a master track. A note outside the engine's playable range SHALL be transposed by whole octaves into range and reported.

#### Scenario: Round trip
- **WHEN** a song is assembled from 8 known notes and rendered
- **THEN** pitch-tracking the render returns the 8 pitches and onsets within 20 ms of the input

#### Scenario: Out of range
- **WHEN** a note is below the engine's lowest playable pitch
- **THEN** it is moved up by whole octaves and listed in the report

### Requirement: Compare
The `compare` stage SHALL render the assembled song, align it to the original stem by cross-correlation of the onset envelope, and write `compare/1` JSON and a PNG with: per-band loudness difference, spectral-centroid correlation, note-onset F1 (50 ms tolerance) and pitch accuracy. Reported metrics SHALL be reproducible from the same files.

#### Scenario: Self comparison
- **WHEN** a song is compared with its own render
- **THEN** onset F1 is 1.0 and the loudness difference is 0 dB in every band
