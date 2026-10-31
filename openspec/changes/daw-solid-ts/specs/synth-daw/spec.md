# Spec Delta

## ADDED Requirements

### Requirement: Hidden panels hold no content
A tab panel that is not shown (piano roll, device, mixer) and a dialog or menu that is closed SHALL hold no controls
or graphics, and SHALL do no display work while playing. Showing it again SHALL present the current song, selection
and playback state, including any edits, undo or redo made while it was hidden.

#### Scenario: Mixer hidden
- **WHEN** the Device tab is shown
- **THEN** the mixer panel contains no elements

#### Scenario: Edits while hidden
- **WHEN** the mixer is hidden, a track is muted from its header, and the Mixer tab is shown again
- **THEN** that track's strip shows Mute pressed

### Requirement: Off-screen arrangement content
The arrangement SHALL create track headers, lanes, clips and automation lanes only for the tracks and bars in view,
plus a margin. Scrolling, zooming, follow, and keyboard navigation SHALL show the content that comes into view. The
total scroll extent SHALL match the whole song.

#### Scenario: Large song
- **WHEN** a song with 50 tracks is open and 12 tracks fit in the view
- **THEN** fewer than 50 track lanes exist, and the scroll height covers all 50 tracks

#### Scenario: Keyboard to an off-screen track
- **WHEN** a track header has focus and Arrow Down is pressed until the next track is below the view
- **THEN** the arrangement scrolls, that track's header exists, has focus, and is selected

### Requirement: Playback display budget
While playing a song with 50 tracks, with the mixer or the device panel shown and the live view on, at least 95 % of
display frames SHALL be under 20 ms apart. A playhead, position, meter, load readout or automated control SHALL be
rewritten only when its shown value changes. Playback SHALL never rebuild a strip, lane or panel.

#### Scenario: Mixer during playback
- **WHEN** the mixer is shown and a 50-track song plays for 5 s
- **THEN** at least 95 % of frame intervals are under 20 ms, and only meter elements change in the mixer

### Requirement: Edit response budget
On a song with 50 tracks, selecting a track, a fader or knob step, mute, solo, and a send level change SHALL each
complete their display update within one 60 Hz frame (16.7 ms) on the reference machine. A single edit SHALL update
only the controls that show the changed values.

#### Scenario: Select a track
- **WHEN** a mixer strip name is clicked on a 50-track song
- **THEN** the selection, device panel and strips are updated within 16.7 ms

#### Scenario: Fader step
- **WHEN** a strip fader is moved by one step
- **THEN** its dB readout and the track header fader show the new value, and no other strip changes
