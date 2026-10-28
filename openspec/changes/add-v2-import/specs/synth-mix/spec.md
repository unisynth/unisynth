# Spec Delta

## ADDED Requirements

### Requirement: Extra device output and input pairs
A device MAY have up to 4 extra stereo output pairs (numbered 1–4, written by the device) and up to 2 extra stereo
input pairs (numbered 1–2, read by the device). A receive SHALL select which pair of the sending track it reads (main,
or an extra output pair of a device on that track) and which pair of the receiving track it writes (main, sidechain or
an extra input pair). A receive whose pair is outside that range SHALL be a load error.

#### Scenario: Unwritten pair
- **WHEN** a receive reads extra output pair 3 and no device on the sending track writes it
- **THEN** the receive adds silence; the devices of a track share its extra pairs, so a later device's write replaces an earlier one's

#### Scenario: Old blob unchanged
- **WHEN** a song blob with receiving channels 0 and 2 is decoded
- **THEN** the receives mean main and sidechain, as before, and the render is bit-identical

#### Scenario: Send output into a reverb bus
- **WHEN** a track's instrument writes its reverb send to output pair 1 and a bus track receives that pair into main at volume 1
- **THEN** the bus input equals the instrument's output pair 1 for that sub-block

#### Scenario: Pair out of range
- **WHEN** a receive reads output pair 5, or writes receiving pair 8
- **THEN** loading returns an error and does not panic

### Requirement: Pair selection is backward compatible
The pair selection SHALL be encoded in the existing receiving-channel field, so that every song blob written before
this change decodes and renders unchanged.

#### Scenario: Old blob bit-identical
- **WHEN** the reference songs and the size-probe songs written before this change are rendered
- **THEN** their output hashes are unchanged

## MODIFIED Requirements

### Requirement: Track render steps
For each sub-block, a track SHALL fire its MIDI lane's events in the sub-block to every device on the track with their offsets, apply its automations, clear its 4 channel buffers and its extra pairs, add each receive (the selected pair of the sending track × volume into channel pair 0 = main, 2 = sidechain, or an extra input pair), run its devices in chain order in place, then scale all 4 buffers and its extra output pairs by the track volume.

#### Scenario: Send into sidechain
- **WHEN** track A sends at volume 0.5 into channel 2 of track B
- **THEN** B's sidechain input equals 0.5 × A's main output for that sub-block, and B's main input is unaffected

#### Scenario: Aux input
- **WHEN** track A's main output is received into input pair 1 of track B at volume 1
- **THEN** B's instrument reads A's output on its aux input and B's main input is unaffected
