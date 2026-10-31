## MODIFIED Requirements

### Requirement: Track render steps
For each sub-block, a track SHALL fire its lane's events to its devices with their offsets, clear its 4 channel buffers and extra pairs, add each receive (the sending track's selected pair × volume into pair 0 = main, 2 = sidechain, or an extra input pair), decide whether it sleeps, apply its automations, then, unless it sleeps, run its devices in chain order in place and scale its buffers and extra output pairs by the track volume.

#### Scenario: Send into sidechain
- **WHEN** track A sends at volume 0.5 into channel 2 of track B
- **THEN** B's sidechain input equals 0.5 × A's main output for that sub-block, and B's main input is unaffected

#### Scenario: Aux input
- **WHEN** track A's main output is received into input pair 1 of track B at volume 1
- **THEN** B's instrument reads A's output on its aux input and B's main input is unaffected

## ADDED Requirements

### Requirement: Quiet tracks sleep
With an engine whose `quiet_skip()` is true and the skip on (the default), a track SHALL skip its devices (its buffers stay zero) in a sub-block when every event delivered to it is due before it, its input is exactly zero, and every device's output and the input have stayed below −120 dBFS for at least the sum of its devices' `quiet_after()` and at least 4096 frames. A device answering `None`, or whose own `quiet_skip()` is false, SHALL keep its track running.

#### Scenario: Echo tail kept
- **WHEN** a tone feeds a delay of 6000 frames on one track
- **THEN** the track does not sleep before the delayed tone has played, and its output equals a render without skipping

#### Scenario: Input wakes a sleeping track
- **WHEN** a sleeping track receives a sounding signal
- **THEN** it runs its devices from that sub-block on

#### Scenario: Unknown memory
- **WHEN** a device on the track answers `quiet_after()` with `None`, or its own `quiet_skip()` is false while the song's first device's is true
- **THEN** the track runs every sub-block

#### Scenario: Early event
- **WHEN** a device with lookahead holds a note delivered for a later sub-block
- **THEN** the track stays awake until the note's frame has passed

### Requirement: A sleeping track wakes in step
A sleeping track's automations SHALL still apply; one that makes a device's `quiet_after()` unknown SHALL make the track run in that sub-block. Before its devices run again, each SHALL get `wake(skipped)` with the frames skipped, before that sub-block's automation.

#### Scenario: Wake before automation
- **WHEN** a track sleeps for 5900 frames and then runs again
- **THEN** its devices get `wake(5900)` after the last sleeping sub-block's automation and before the waking sub-block's

#### Scenario: Automation makes it audible
- **WHEN** a sleeping track's automation raises a parameter that turns silence into sound
- **THEN** the track sounds from that sub-block on, as without skipping
