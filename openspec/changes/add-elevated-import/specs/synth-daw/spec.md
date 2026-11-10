# Spec Delta

## ADDED Requirements

### Requirement: Effect modes by stored position
The DAW SHALL decode an effect's mode through the engine's stored positions: each effect table's mode parameter
defaults to the mode's stored value, and its options list every position, with "—" at a free one. A device whose
stored value selects a mode SHALL show that mode's table. Adding an effect SHALL write its mode's stored value. The
mode selector SHALL never offer a free position.

#### Scenario: Expanded distortion
- **WHEN** a song holds a distortion device whose mode value is 5/6 (knee, position 5 of 7)
- **THEN** its panel shows "Distortion · Knee" with the knee's threshold, ratio and offset

#### Scenario: Earlier value
- **WHEN** a song written before add-elevated-import holds a distortion with mode value 1/3 (curve of four modes)
- **THEN** the panel shows "Distortion · Curve"

### Requirement: Elevated fields in the device panel
A Pair slot SHALL show its wave, phase shift, detune and polarity, with a drawing of its cycle. The voice SHALL show
the pitch drop's base (Hz) and rate (as the time to fall halfway, "off" at 0). Each envelope card SHALL show `linear`
and `hold`, and a linear envelope's attack, decay and release SHALL read in its own law (`5000·p²` ms, no 1 ms floor).

#### Scenario: Converted Atrium
- **WHEN** the converted Atrium opens on the Device tab
- **THEN** Osc 1 shows "Pair" and its four controls, Env 1 shows linear "On" and its hold, and the first track's effects
  read Knee, Sine, SVF LFO and Ping-pong
