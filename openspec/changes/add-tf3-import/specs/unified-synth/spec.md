# Spec Delta

## ADDED Requirements

### Requirement: Point oscillator
With feature `us-osc-points`, an oscillator slot of type `points` SHALL build a 256-sample cycle from the fixed points
(0, 0) and (1, 0) and 1–4 user points. Each user point's offset is placed in the span left after the previous point
(clamped to 0.01–0.99 of it) and its value is `2p − 1`. Each segment is interpolated by its own mode: step, linear or
spline.

#### Scenario: Step cycle
- **WHEN** a `points` slot has one point at offset 0.5, value 1, step mode on both segments
- **THEN** the cycle is 0 for the first half and 1 for the second half (within one sample of the boundary)

### Requirement: Point oscillator playback
A `points` cycle SHALL be rebuilt at a control segment only when a point input (after the mod matrix) changed. It
SHALL be played with the wavetable slot's layers, left/right detune, spread, drive and hard clip, reading without
interpolation.

#### Scenario: Modulated point
- **WHEN** an LFO Scale slot targets the slot's point-1 value
- **THEN** the cycle changes between control segments as the LFO moves, and stays fixed while the LFO output is constant

#### Scenario: Missing module
- **WHEN** a patch with a `points` slot loads in a build without `us-osc-points`
- **THEN** the load fails with `UnsupportedModule` naming the point oscillator

### Requirement: Tunefish 3 decimation stage
A `points` or `pad` slot SHALL always render through Tunefish 3's decimation stage: at k times the song rate, k the
smallest integer with `k·rate ≥ 64·f` for the voice's note frequency f (at most 8), its output scaled by the slot gain
passes two cascaded ladder low-passes at 12.83 kHz designed at that rate, and k samples are averaged, also when k = 1.

#### Scenario: High note
- **WHEN** a sawtooth-like `points` cycle plays note 100
- **THEN** its octave bands above 10 kHz are lower than those of the same cycle read once per frame without the stage

#### Scenario: Low note still filtered
- **WHEN** a `points` square plays note 45 (k = 1)
- **THEN** its 16 kHz octave band is lower than without the stage

### Requirement: PAD oscillator
With feature `us-osc-pad`, an oscillator slot of type `pad` SHALL play a static PADsynth table of 2^16 samples. The
table is built once at load from the slot's bandwidth, bandwidth scale, damp, harmonics and profile (single, detune,
spread, gauss). These values SHALL be structural (`set_param` ignores them). The slot advances `f/260` table periods
per sample, so its fundamental sounds at 0.499·f, without interpolation, with left/right detune, drive and hard clip.

#### Scenario: Spectrum
- **WHEN** a `pad` slot with 8 harmonics and the gauss profile plays note 69
- **THEN** the output's spectrum peaks at the harmonics of 219.6 Hz (440 · 0.499), and each peak's width grows with the bandwidth value

#### Scenario: Automation ignored
- **WHEN** `set_param` changes a `pad` slot's bandwidth during playback
- **THEN** the output is identical to the output without that call

### Requirement: Shared PAD table memory
A PAD table SHALL be allocated once per `pad` slot and instrument, not per voice. `need` SHALL count it once.

#### Scenario: Need with 16 voices
- **WHEN** `need` is asked for a patch with one `pad` slot and capacity 16, then capacity 1
- **THEN** both results differ only by the per-voice size of 15 voices, and each includes one table of 2^16 f32

### Requirement: New oscillator type storage
The `points` and `pad` types SHALL be stored at free positions of the expanded oscillator-type choice. Stored values
of the existing types keep their meaning. The point oscillator's point count, interpolation modes, layers, layer
detune and layer spread SHALL be appended parameters after the current last index.

#### Scenario: Existing songs unchanged
- **WHEN** the existing golden songs are rendered in a build with `us-osc-points` and `us-osc-pad`
- **THEN** they are byte-identical to their goldens

#### Scenario: Round trip
- **WHEN** a patch with a `points` slot and a `pad` slot is encoded and parsed again
- **THEN** both slot types, the point count, every interpolation mode and the layer settings decode unchanged

### Requirement: Tunefish 3 filter form
A filter slot's `form` set to 1 on the RBJ high-pass, RBJ band-pass and notch types SHALL select Tunefish 3's form:
the band-pass with `α = sin ω0/2 · √((A + 1/A)(1/BW − 1) + 2)`, `A = 1.059253725`, `BW = 1 − clamp(q, 0, 0.95)` and
constant skirt gain; the high-pass with resonance up to 0.95; the notch as an RBJ notch with the same α and no latency.
Form 0 SHALL keep these types as they were. The form SHALL require `us-flt-feedback`.

#### Scenario: Wide band-pass
- **WHEN** an RBJ band-pass at 1 kHz with form 1 and resonance 0.5 plays a 1 kHz sine
- **THEN** its gain is `1/√((A + 1/A) + 2)` within 1 %

#### Scenario: Stored songs unchanged
- **WHEN** the project's songs and patches load
- **THEN** none sets form on an RBJ or notch slot, and the reference songs render bit-identically

#### Scenario: Build without the forms
- **WHEN** a build without `us-flt-feedback` loads a patch with an RBJ slot in form 1
- **THEN** the load fails with `UnsupportedModule` naming `us-flt-feedback`

### Requirement: One-tap chorus mode
The chorus kind SHALL have a second mode, one tap, behind `us-fx-chorus-tf3`: per channel `y = x + gain·x[n − d]`,
`d = 1 + (sin(t·π·rate/sr) + 1)/2 · fi(50·depth²)` ms rounded to frames, on the song's absolute frame. A chorus
stored before this mode existed SHALL keep mode 0.

#### Scenario: Impulse
- **WHEN** an impulse passes the one-tap chorus at depth 0 and gain 0.5
- **THEN** the dry impulse is unchanged and one copy at 0.5 follows 44 frames later at 44.1 kHz

#### Scenario: Split render
- **WHEN** a render is split into calls of other sizes
- **THEN** the output is identical

