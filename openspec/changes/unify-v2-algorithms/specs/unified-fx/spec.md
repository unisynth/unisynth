# Spec Delta

## RENAMED Requirements

- FROM: `### Requirement: V2 effect modes`
- TO: `### Requirement: Comb reverb, modulated delay, look-ahead compressor and cut EQ modes`

## MODIFIED Requirements

### Requirement: Comb reverb, modulated delay, look-ahead compressor and cut EQ modes
The effects SHALL have these modes:
- reverb `comb`: parallel combs with damping, then all-passes, with a low cut
- delay `modulated`: a modulated stereo delay
- compressor `lookahead`: peak or RMS detection, look-ahead, stereo link
- EQ `cut`: a low cut and a high cut

Their parameters SHALL use 7-bit laws (value = step / 128). These four modes SHALL have no format-version (era)
parameter: version-dependent behaviour is written by the converter (low cut off) or is the mode's own DC-bias flag.
The other modes and their parameter layouts SHALL be unchanged.

#### Scenario: Existing modes unchanged
- **WHEN** the effect self-goldens recorded before this change are run
- **THEN** every hash still matches

#### Scenario: Reverb low cut by format version
- **WHEN** a song converted from a V2 format version below 4 uses the comb reverb
- **THEN** the converter sets the low cut to off, so the reverb runs without it, as V2 did before version 4

### Requirement: V2 modes match the reference
Each of the comb reverb, modulated delay, look-ahead compressor and cut EQ modes SHALL match its V2 original within
the parity tolerances (1 dB integrated loudness, 3 dB per octave band 63 Hz–16 kHz, onsets matched) on a synthetic
test song rendered by the reference player and, converted, by the unified effect.

#### Scenario: Delay parity
- **WHEN** a song with one plucked note into the V2 delay at full feedback is rendered both ways
- **THEN** the parity report passes
