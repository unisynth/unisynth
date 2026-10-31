## MODIFIED Requirements

### Requirement: Quiet-track answers
With feature `us-quiet` the unified devices SHALL allow the quiet-track skip. The instrument SHALL answer `quiet_after()` 0 only without sounding voices, queued events, channel stage and speech; the shaper, EQ 3band and a wrap without offset 0; the tempo delay and the comb their line capacity; the freeverb reverb, unless frozen, its core's sleep span plus its pre-delay line; every other mode `None`.

#### Scenario: Converted songs unchanged
- **WHEN** the three converted TBL songs render with and without the skip
- **THEN** they match within one 16-bit step

#### Scenario: Freeverb memory
- **WHEN** a freeverb reverb is asked `quiet_after()`
- **THEN** it answers at least 8192 frames plus its pre-delay line, and `None` while frozen
