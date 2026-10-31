## ADDED Requirements

### Requirement: Per-note bytes as latched lanes
The mini-cauldron converter SHALL write the retrigger, phase and age bytes so that each note's byte decodes exactly from its step until `LATCH_WINDOW` frames later (or the next step), with as few points as a straight line between those windows allows; pitch bytes SHALL stay exact steps.

#### Scenario: Rising retrigger count
- **WHEN** a record's retrigger byte rises by 2 on each of 50 evenly spaced notes
- **THEN** its lane has at most 4 points and every note reads its own byte
