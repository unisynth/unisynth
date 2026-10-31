# synth-mix Specification

## Purpose
One song and mixer model for all engines, so a single intro song can mix instruments and effects from WaveSabre, the
Tunefish-compatible engine and later engines. Its binary format is WaveSabre's song blob (as written by
`WaveSabreConvert`), so existing WaveSabre songs load unchanged.

## Requirements

### Requirement: Song header and devices
The decoder SHALL read the WaveSabre song blob (little endian) as written by `WaveSabreConvert`: tempo (i32), sample rate (i32), length in seconds (f64), then the device count and per device a kind id byte, a chunk size (i32) and the chunk bytes. It SHALL decode in place from borrowed bytes.

#### Scenario: Round trip
- **WHEN** a song blob built in the test (3 devices, 2 lanes, 3 tracks, 2 automations) is decoded
- **THEN** every field equals the values used to build it

#### Scenario: Malformed input
- **WHEN** the blob is truncated at any byte, or has a device index, lane id, sending track or param id out of range
- **THEN** decoding returns an error and does not panic

### Requirement: MIDI lanes
After the devices, the decoder SHALL read the lane count and per lane an event count and per event a delta time in samples (i32) and a note byte. If the note byte's top bit is clear it is a note-on followed by a velocity byte; if set it is a note-off with no velocity byte. Event arrays SHALL NOT be copied.

#### Scenario: Note-off without velocity
- **WHEN** a lane holds note-on 60 vel 100 at delta 0 and note-off 60 at delta 1000
- **THEN** it decodes as 6 + 5 bytes of events and the next field starts right after them

### Requirement: Tracks and automation encoding
After the lanes, the decoder SHALL read the track count and per track: volume (f32); receives (count, then sending track i32, receiving channel i32, volume f32); device indices (count, then i32 each); MIDI lane id (i32); automations (count, then device index within the track, param id, point count, and per point a delta time in samples i32 and a value byte).

#### Scenario: Automation points
- **WHEN** an automation has points at deltas 0 and 500 with bytes 0 and 255
- **THEN** it decodes to absolute times 0 and 500 with values 0.0 and 1.0

### Requirement: Device kind registry
Device kinds SHALL be numeric ids mapped to constructors at compile time. Ids 0–12 SHALL be WaveSabre's (0 Falcon,
1 Slaughter, 2 Thunder, 3 Scissor, 4 Leveller, 5 Crusher, 6 Echo, 7 Smasher, 8 Chamber, 9 Twister, 10 Cathedral,
11 Adultery, 12 Specimen). Other engines SHALL register ids from 32 upward. A kind whose cargo feature is disabled SHALL be
reported as unsupported when the song loads, not when it renders.

#### Scenario: Feature-disabled kind
- **WHEN** a song uses Cathedral and the crate is built without `ws-cathedral`
- **THEN** loading returns an unsupported-device error naming kind 10

### Requirement: Track render steps
For each sub-block, a track SHALL fire its lane's events to its devices with their offsets, clear its 4 channel buffers and extra pairs, add each receive (the sending track's selected pair × volume into pair 0 = main, 2 = sidechain, or an extra input pair), decide whether it sleeps, apply its automations, then, unless it sleeps, run its devices in chain order in place and scale its buffers and extra output pairs by the track volume.

#### Scenario: Send into sidechain
- **WHEN** track A sends at volume 0.5 into channel 2 of track B
- **THEN** B's sidechain input equals 0.5 × A's main output for that sub-block, and B's main input is unaffected

#### Scenario: Aux input
- **WHEN** track A's main output is received into input pair 1 of track B at volume 1
- **THEN** B's instrument reads A's output on its aux input and B's main input is unaffected

### Requirement: Track order and master
Tracks SHALL render in index order, skipping a track until all tracks it receives from have rendered in this sub-block (the order upstream's single-threaded renderer produces). The last track SHALL be the master, and the song output SHALL be its main L/R.

#### Scenario: Dependency order
- **WHEN** a track is listed before a track it receives from
- **THEN** the sending track still renders first in each sub-block

### Requirement: Automation
Automation SHALL set a device parameter once per render block, at the block start. The value is linear interpolation
between the surrounding points, held at the first point before it and at the last point after it, with values
`byte / 255`.

#### Scenario: Interpolated point
- **WHEN** points are (0 samples, 0) and (1000 samples, 255) and a block starts at sample 500
- **THEN** the parameter is set to 0.5 for that block

### Requirement: Render API
The song player SHALL expose `new(song, pool)`, `required_pool(song)`, `render_f32(out, frames)`, `render_i16(out, frames)` (interleaved stereo; i16 = clamp(trunc(x·32767))), `length_frames()` and `seek(frame)`, which re-renders silently from the start to restore device state.

#### Scenario: Seek
- **WHEN** the player seeks to frame f and renders n frames
- **THEN** the output equals frames f..f+n of a render from the start

### Requirement: Fixed internal sub-blocks
The player SHALL render internally in sub-blocks of a configurable size (default 100 frames, the upstream players' step), independent of the caller's buffer size, so event and automation granularity never depend on the caller.

#### Scenario: Caller buffer size independence
- **WHEN** a song renders with caller buffers of 64 frames and of 44100 frames
- **THEN** both outputs are bit-identical

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
