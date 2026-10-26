# Spec Delta

## Purpose
A WebAssembly engine that plays a `song/1` song with the unified synth in real time while it is being edited, and
sounds the same as the intro player whenever no edit happened since playback started.

## ADDED Requirements

### Requirement: Same sound as the player
Started at frame 0 on a freshly built song with no edits during playback, the live host's output SHALL be
bit-identical to `synth_core::mix::Player` rendering the exported blob, for any sequence of render call sizes.

#### Scenario: Equivalence
- **WHEN** a test song with notes, automation, a sidechain receive and three tracks is rendered by the live host in
  128-frame calls and by the player in one call
- **THEN** the two outputs are bit-identical

### Requirement: Render in fixed-size calls
The host SHALL render interleaved stereo f32 into a buffer it owns, for any call size up to 4096 frames, without
allocating memory during a render call. It SHALL keep the player's 100-frame internal sub-block, sample-exact note
offsets and automation applied at each sub-block start.

#### Scenario: No allocation while rendering
- **WHEN** the host renders 10 s of a song in 128-frame calls under an allocation counter
- **THEN** no allocation happens inside a render call

### Requirement: Edits while playing
Between render calls the host SHALL accept a new schedule for one track, a parameter value for one device, a new
device chain for one track, and a new routing. Each applies at the next sub-block start. A schedule swap SHALL end
that track's sounding notes; a chain rebuild SHALL restart only that track's voices. No edit SHALL panic, read out of
bounds or stop the audio.

#### Scenario: Knob while playing
- **WHEN** the filter cutoff of a playing instrument is changed
- **THEN** the change is audible from the next 100-frame sub-block and the other tracks are not affected

#### Scenario: Note edit ahead of the playhead
- **WHEN** a note one bar ahead of the playhead is moved up an octave
- **THEN** it plays at the new pitch when the playhead reaches it

### Requirement: Transport
The host SHALL support play from a frame, stop (all notes off, effect tails continue for up to 2 s unless stopped
twice), loop between two frames (the jump happens at the loop end sample) and seek. Seek SHALL start at the target
frame without silent pre-roll; notes that started before the target and are still held SHALL be re-triggered at the
target ("chase").

#### Scenario: Loop
- **WHEN** the loop is bars 5–9 and the playhead reaches the end of bar 8
- **THEN** the next frame rendered is the first frame of bar 5 and held notes are re-chased

### Requirement: Audition
The host SHALL accept note-on/note-off for one track's instrument outside the schedule (computer keyboard or MIDI),
sounding at the next render call, both while playing and while stopped.

#### Scenario: Audition while stopped
- **WHEN** transport is stopped and an audition note-on arrives for track `lead`
- **THEN** the next render call contains that instrument's note

### Requirement: Meters and load
After each render call the host SHALL report per-track and master peak levels (L/R) and the render time of the last
call as a fraction of the call's real-time duration.

#### Scenario: Silent track
- **WHEN** a track has no notes
- **THEN** its peak level is 0

### Requirement: Parameter tables
The host SHALL export, for every device kind it contains, the parameter list: index, name, group, default, law with
physical unit and display range, and for choice parameters the option names; plus which parameters are structural
(need a device rebuild). The UI and validation SHALL use only these tables, so a newly added kind needs no UI code.

#### Scenario: Physical display
- **WHEN** the table entry for a filter cutoff is mapped at 0.5
- **THEN** it reports 5015 Hz (law `20 + 19 980 p²`)
