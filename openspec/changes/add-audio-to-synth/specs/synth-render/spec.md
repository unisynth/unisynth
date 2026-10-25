# Spec Delta

## Purpose

A headless renderer over the shared synth layer, used by the fitter and for A/B comparison.

## ADDED Requirements

### Requirement: Single-note render
`synth-render note` SHALL take an engine, a device kind, a normalized parameter vector (each value in 0..1), a MIDI note, velocity, note length and tail length, and render mono-compatible stereo at 44 100 Hz to a wav file or stdout (f32). An out-of-range or non-finite parameter SHALL be rejected with an error and no output. The same inputs SHALL give byte-identical output.

#### Scenario: Deterministic
- **WHEN** the same note request is rendered twice
- **THEN** the outputs are identical

#### Scenario: Bad parameter
- **WHEN** a parameter value is 1.5 or NaN
- **THEN** the command fails, names the parameter index and writes nothing

### Requirement: Batch mode
`synth-render batch` SHALL read newline-delimited JSON note requests on stdin and write length-prefixed f32 buffers on stdout in the same order, rendering requests on a persistent engine instance without per-request process start-up. Each request SHALL use a fresh device state, so results do not depend on the previous request.

#### Scenario: Order independence
- **WHEN** requests A, B are sent as A,B then as B,A
- **THEN** the buffer for A and for B are identical in both runs

### Requirement: Song render
`synth-render song` SHALL render a `synth-mix` blob to a stereo wav, and SHALL report peak, RMS and clipped-sample count as JSON on stderr.

#### Scenario: Clipping report
- **WHEN** a song renders with samples above 1.0
- **THEN** the clipped count is non-zero and the wav is written unclipped f32

### Requirement: Engine selection
Engines SHALL be cargo features. Requesting an engine the binary was built without SHALL fail with the list of compiled engines. A built-in `reference` engine (one saw oscillator + lowpass + ADSR, seven parameters) SHALL always be available for pipeline tests.

#### Scenario: Absent engine
- **WHEN** `--engine tunefish` is used on a build without `engine-tunefish`
- **THEN** the command exits non-zero listing the available engines
