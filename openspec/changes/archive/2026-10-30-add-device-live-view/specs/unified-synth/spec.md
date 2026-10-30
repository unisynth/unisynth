# Spec Delta

## ADDED Requirements

### Requirement: Read-only voice probe
A host-only cargo feature SHALL let a caller read the instrument's voice state at the end of a render call: per active
voice its note, amplitude, release state, envelope levels and stages, LFO phases, sampler play positions and filter
cutoffs, plus per-filter input/output taps summed over voices when enabled at run time. The probe SHALL only read.
Builds without the feature SHALL be unchanged in code and output.

#### Scenario: Goldens with the probe
- **WHEN** the instrument goldens run with the probe feature on and taps enabled
- **THEN** every golden hash matches the build without the feature

#### Scenario: Player size
- **WHEN** the compo player is built
- **THEN** it does not enable the probe feature and its size is unchanged

#### Scenario: Envelope state
- **WHEN** a note is held past attack and decay with sustain 0.5 and the probe is read
- **THEN** the voice reports envelope 1 in the sustain stage at level 0.5
