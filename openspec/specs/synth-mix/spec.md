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
For each sub-block, a track SHALL fire its MIDI lane's events in the sub-block to every device on the track with their offsets, apply its automations, clear its 4 channel buffers, add each receive (sending track's main L/R × volume into channel pair 0 = main or 2 = sidechain), run its devices in chain order in place, then scale all 4 buffers by the track volume.

#### Scenario: Send into sidechain
- **WHEN** track A sends at volume 0.5 into channel 2 of track B
- **THEN** B's sidechain input equals 0.5 × A's main output for that sub-block, and B's main input is unaffected

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
