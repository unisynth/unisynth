## ADDED Requirements

### Requirement: Quiet-track answers
With feature `us-quiet` the unified devices SHALL allow the quiet-track skip. The instrument SHALL answer `quiet_after()` 0 only without sounding voices, queued events, channel stage and speech; the shaper, EQ 3band and a wrap without offset 0; the tempo delay and the comb their line capacity; every other mode `None`.

#### Scenario: Converted songs unchanged
- **WHEN** the three converted TBL songs render with and without the skip
- **THEN** they match within one 16-bit step

### Requirement: Woken effects resume in step
EQ 3band's `wake` SHALL restart its gain ramp from the parameters as set while it slept; the comb's SHALL advance its sweep by the frames skipped.

#### Scenario: Woken EQ
- **WHEN** an EQ 3band sleeps through a gain change and wakes before the next one
- **THEN** its next block ramps exactly as if it had run

#### Scenario: Woken comb
- **WHEN** a comb with a sweep sleeps 5000 frames and wakes
- **THEN** its output equals a comb that ran those 5000 frames of silence
