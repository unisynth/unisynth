## ADDED Requirements

### Requirement: Cauldron filter latched at note-on
A cauldron slot SHALL take its filter settings (base and amount after the retrigger byte, decay, damping) at a non-legato note-on and keep them until the next one.

#### Scenario: Retrigger byte changed mid-note
- **WHEN** the retrigger byte changes while a cauldron note sounds
- **THEN** the note's filter is unchanged, and the next non-legato note-on takes the new value
