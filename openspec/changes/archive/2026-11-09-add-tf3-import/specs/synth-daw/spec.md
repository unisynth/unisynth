## ADDED Requirements

### Requirement: Points and PAD table oscillator blocks
The device panel SHALL show a Points slot's points as one column per point (offset, value, the mode of the segment
ending at it) plus the last segment's mode, following the slot's point count, with a screen drawing the cycle the
engine builds. A PAD table slot SHALL show bandwidth, scale, damp, harmonics, profile (by name) and drive, with a
screen sketching its partials.

#### Scenario: Point count
- **WHEN** a Points slot's point count is set to 4
- **THEN** the slot shows four point columns and the last-segment column, and its screen marks four points

#### Scenario: Cycle drawing
- **WHEN** the panel draws a Points slot
- **THEN** the drawn cycle equals the engine's `pts::cycle` for the same values within 1e-4

#### Scenario: PAD profile name
- **WHEN** a PAD table slot's profile knob is at its maximum
- **THEN** its value text is "gauss"

### Requirement: PAD table inputs rebuild the device
A PAD slot's table inputs (bandwidth, scale, damp, harmonics, profile) SHALL take effect by rebuilding the device, as
structural values do, because the table is built when the instrument loads. They SHALL NOT be automation targets.
Drive SHALL stay a live value.

#### Scenario: Live host
- **WHEN** a song update changes a PAD slot's bandwidth
- **THEN** the live host rebuilds that track's device; a change of the slot's drive is a parameter set

#### Scenario: Knob drag
- **WHEN** the user drags a PAD table input knob
- **THEN** the DAW sends the song to the engine once, when the knob is released, and the parameter is not offered as an automation lane

### Requirement: Tunefish 3 presets source
The instrument library SHALL show "Tunefish 3 presets" after the Tunefish 4 programs, tagged by program number, and
load a converted preset like a converted Tunefish 4 program (instrument plus its insert effects).

#### Scenario: Listed and loadable
- **WHEN** the catalogue holds converted Tunefish 3 presets
- **THEN** the source lists them as `#n` entries that load into the selected track

