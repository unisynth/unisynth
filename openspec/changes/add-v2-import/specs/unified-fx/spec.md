# Spec Delta

## ADDED Requirements

### Requirement: V2 effect modes
The effects SHALL gain the modes reverb `v2`, delay `v2` (modulated stereo delay), compressor `v2` and EQ `cut` (V2's
post low cut and high cut). Each SHALL take V2's parameters in V2's order (value = V2 raw value / 128) with V2's laws.
Existing modes and their parameter layouts SHALL be unchanged.

#### Scenario: Existing modes unchanged
- **WHEN** the effect self-goldens recorded before this change are run
- **THEN** every hash still matches

#### Scenario: Reverb low cut by format version
- **WHEN** a song converted from a format version below 4 uses the V2 reverb
- **THEN** the reverb runs without its low-cut stage, as V2 did before version 4

### Requirement: V2 modes match the reference
Each V2 mode SHALL match V2 within the parity tolerances (1 dB integrated loudness, 3 dB per octave band 63 Hz–16 kHz,
onsets matched) on a synthetic test song rendered by the reference player and, converted, by the unified effect.

#### Scenario: Delay parity
- **WHEN** a song with one plucked note into the V2 delay at full feedback is rendered both ways
- **THEN** the parity report passes
