# synth-daw Specification

## Purpose
A browser DAW for composing intro music with the unified synth: arrange clips, edit notes, shape patches and mix while
hearing the intro's own synth, and save the result as a `song/1` file in the project.

## Requirements

### Requirement: Serving and file access
The local server SHALL serve the DAW at `/daw` and a file API `/api/music` that lists, reads and writes `*.song.json`,
`*.patch.json` and exported `*.song.bin` files under `project/music/` only. It SHALL apply the same rules as
`/api/project`: loopback only, per-server token, foreign `Origin` refused, paths resolved and confined (no `..`, no
symlink escape), a 16 MB body limit, and write only after the song validates.

#### Scenario: Path escape
- **WHEN** a client asks to read `../intro.json` or a symlink pointing outside `project/music/`
- **THEN** the server refuses with 403 and reads nothing

### Requirement: Read-only library and bank routes
The server SHALL serve read-only `/api/library` (the catalog of library sources) and `/api/bank` (GM bank bytes from
`GM_DLS` or the OS bank path, 404 when absent) under the same token and Origin rules.

#### Scenario: No bank
- **WHEN** no GM bank exists on the machine
- **THEN** `/api/bank` answers 404 and the DAW shows GM entries as unavailable

### Requirement: Open, save and export
The DAW SHALL open a song from the music list, create a new song (one instrument track and master), import a song blob,
save with Ctrl+S or the Save button (writes `song/1`), and export the blob next to it. Unsaved changes SHALL show in
the title and on the Save button, and leaving the page with unsaved changes SHALL ask first. Validation errors SHALL
show with their JSON paths and block saving.

#### Scenario: Dirty state
- **WHEN** a note is added
- **THEN** the title shows an unsaved marker until the song is saved

### Requirement: Switching songs
Opening another song, New song or Import with unsaved changes SHALL ask "Save changes to <song>?" with Save, Don't
save and Cancel. Each song keeps its own tempo, time signature, loop and length. Playback and automation shown at the
playhead never count as changes.

#### Scenario: Discard and return
- **WHEN** the user mutes Kick, opens outro.song.json, chooses Don't save, then opens demo.song.json again
- **THEN** Kick is not muted and the title has no "•"

### Requirement: Transport bar
The DAW SHALL show play/stop, loop on/off with its range, position as bars.beats.ticks and as m:ss.mmm, tempo, time
signature, follow-playhead toggle, snap setting and engine load. Space SHALL toggle play, and clicking the ruler SHALL
seek.

#### Scenario: Seek by ruler
- **WHEN** the user clicks the ruler at bar 9 while stopped and presses Space
- **THEN** playback starts at bar 9.1.0

### Requirement: Loop editing
The loop range SHALL be editable in the ruler's top strip: drag the loop bar to move it, drag an edge to resize it,
drag on empty strip to draw a new loop, all snapped to the current snap. The loop SHALL be off when the DAW opens. The transport Loop field SHALL accept a typed
"start – end" in bars, the Loop button or L SHALL toggle it, and Ctrl+L SHALL set it to the selected clips. The loop
shading in the lanes and the piano roll SHALL follow every change.

#### Scenario: Resize the loop
- **WHEN** the loop is 9–17, snap is Bar, and the user drags its right edge to bar 21
- **THEN** the loop is 9–21, the field shows "9 – 21" and playback wraps at bar 21

#### Scenario: Loop the selection
- **WHEN** clips covering bars 1–25 are selected and the user presses Ctrl+L
- **THEN** the loop becomes 1–25 and is switched on

### Requirement: Song length
The song length SHALL grow by itself to cover the last clip whenever a paste, duplicate, move or resize ends past it,
and the status SHALL say so. The view SHALL show 8 bars past the end, shaded, with an end marker, so clips can be
placed there. A Length field SHALL show the length and accept a new one, but never shorter than the last clip.

#### Scenario: Paste past the end
- **WHEN** three clips spanning 24 bars are pasted at bar 35 of a 32-bar song
- **THEN** they are placed, the song becomes 58 bars and the status says "song extended to 58 bars"

#### Scenario: Length cannot cut clips
- **WHEN** the last clip ends at bar 37 and the user types 20 into Length
- **THEN** the length stays 36 and the status explains why

### Requirement: Playback display
While playing, the playhead SHALL move in the ruler, lanes and piano roll, the position and time SHALL update, the
loop SHALL wrap, follow SHALL keep the playhead in view, clips under the playhead SHALL be marked, automation lane
values SHALL show the value at the playhead, and mixer meters SHALL show the reported levels. The first Stop SHALL
stop in place; a second Stop SHALL return to the loop start (or bar 1 with the loop off). An edit while playing SHALL
show "edited since play".

#### Scenario: Wrap
- **WHEN** the loop is 9–10 and play starts at bar 9.3
- **THEN** after about 1.5 s at 128 BPM the position reads 9.2.x and the roll playhead has jumped back

### Requirement: Zoom
The arrangement SHALL zoom horizontally (6–400 px per bar) and the piano roll in time (3–120 px per 1/16) and in pitch
(6–32 px per key) with Ctrl/⌘+wheel and trackpad pinch (Alt+wheel for pitch), the wheel over the ruler, the − / +
buttons, and the − / + keys in the focused area. Zoom SHALL keep the bar or note under the pointer in place.

#### Scenario: Pinch keeps the anchor
- **WHEN** the user pinches in over bar 9 until bars are about 98 px wide
- **THEN** bar 9 is still under the pointer and clips, loop and automation are redrawn at the new width

### Requirement: Zoom to fit and hit-testing
⤢ (Shift+Z) SHALL fit the selected clips, or the whole song, in the arrangement, and the open clip and its notes in the
roll. Every editor (clips, loop, automation, notes, velocity) SHALL hit what is drawn at any zoom, and ruler numbers
thin out when bars get narrow.

#### Scenario: Draw after zooming the roll
- **WHEN** the roll is zoomed to 150 % time and 160 % pitch and the user double-clicks at the G2 row
- **THEN** a note G2 is added at the clicked step

### Requirement: No accidental text selection
Dragging anywhere in the editor chrome (ruler, lanes, headers, roll, mixer) SHALL NOT select page text or drag images;
text fields stay selectable.

#### Scenario: Loop drag selects nothing
- **WHEN** the user drags the loop across the ruler's bar numbers
- **THEN** the loop moves and no text is selected

### Requirement: Follow and manual scrolling
Follow (on by default) SHALL page the view when the playhead reaches its right edge. A hand scroll or zoom pauses it
(⇥ shown dashed) and the view stays where the user put it; ⇥ or F, starting play, seeking, or the playhead reaching
the right edge of that view SHALL resume it. Track headers and piano-roll keys SHALL mirror their scroller.

#### Scenario: Paused follow resumes at the edge
- **WHEN** the user scrolls the lanes a little while playing and the playhead then reaches the right edge of the view
- **THEN** Follow resumes and the view pages along with the playhead

#### Scenario: Headers stay aligned
- **WHEN** the user momentum-scrolls the lanes vertically
- **THEN** the lanes are the only vertical scroller; headers mirror them, never write back, and stay aligned

#### Scenario: Scroll away while playing
- **WHEN** the arrangement is zoomed in, playing with Follow on, and the user swipes right past the playhead
- **THEN** the view stays at the new position, ⇥ shows paused, and pressing F brings the playhead back into view

#### Scenario: Fast scroll
- **WHEN** a burst of 25 trackpad scroll events moves the lanes right and down
- **THEN** every scroll position is at or past the previous one and the headers line up with their lanes

### Requirement: Arrangement view
The DAW SHALL show one row per track with a header (name, colour, mute, solo, audition-arm, volume) and the track's
clip placements on a bar grid with zoom and horizontal scroll. The user SHALL be able to create, move, resize (crop or
loop), duplicate (Ctrl+D), link, make unique and delete clips, and add, rename, reorder and delete tracks. Linked
placements SHALL be marked visibly.

#### Scenario: Make unique
- **WHEN** a linked placement is made unique and its notes are changed
- **THEN** the other placements of the original clip do not change

### Requirement: Track commands
A track menu (right-click or Shift+F10 on a header) and keys SHALL add an instrument or bus track, rename (F2 or
double-click the name), duplicate, move (Alt+↑/↓) and delete (Ctrl+Delete) tracks; renaming updates sends that target
the track. An empty or duplicate name SHALL be marked on the header at once. The last instrument track and the master
cannot be deleted. Each command is one undo step.

#### Scenario: Delete and restore
- **WHEN** the user deletes the Arp track from its menu and presses Ctrl+Z
- **THEN** Arp returns at its position with its clips, devices and sends

### Requirement: Clip commands
Double-clicking empty lane space SHALL create a one-bar clip there and open it in the piano roll; double-clicking a
clip SHALL open it. A clip menu SHALL offer open, duplicate, duplicate linked, make unique, loop on/off, cut, copy and
delete. Alt+drag of a clip's right edge SHALL loop its pattern (⟳, a marker at each repeat); a plain drag crops or
extends. A looping clip SHALL repeat its notes in playback, export and bounce.

#### Scenario: Loop a clip
- **WHEN** the user Alt-drags the 4-bar clip piano-a to 6 bars
- **THEN** it shows ⟳ with a marker at bar 4 and the roll reads "⟳ 4-bar loop ×1.5"

### Requirement: Clip selection
In the arrangement, click SHALL select a clip, Shift-click SHALL toggle it, dragging on empty lane space or on a
clip's body SHALL select every clip the rectangle touches (across tracks), Ctrl+A SHALL select all and Esc SHALL
clear. A clip moves only by its title bar. Clicking empty space or a clip body SHALL also set the insert cursor
(snapped) and select that track.

#### Scenario: Range on a full lane
- **WHEN** Kick is covered by clips and the user drags from inside a Kick clip body down into Bass
- **THEN** the touched clips are selected and none moves

#### Scenario: Range across tracks
- **WHEN** the user drags a rectangle over bars 1–8 of Kick and Bass
- **THEN** the clips of both tracks in that range are selected and the status says how many

### Requirement: Selecting from track headers
Shift- or Ctrl-click on a track header SHALL add or remove all of that track's clips, Ctrl+A on a focused header SHALL
select its clips, and the clip editing shortcuts SHALL work while a header has focus.

#### Scenario: Copy a whole track
- **WHEN** the user Shift-clicks the Lead header and presses Ctrl+C
- **THEN** all Lead clips are copied

### Requirement: Clip editing
Selected clips SHALL move by drag (snapped, also onto another instrument track) or by Arrow Left/Right (snap, Shift = 4
bars) and Arrow Up/Down (adjacent instrument track), resize by dragging their right edge, and be cut, copied, pasted
at the insert cursor (Ctrl+V: new unique clips; Ctrl+Shift+V: linked placements), duplicated after the selection
(Ctrl+D unique, Ctrl+Shift+D linked) and deleted. Copied clips keep their relative tracks and times.

#### Scenario: Paste keeps layout
- **WHEN** clips on Kick (bar 1) and Bass (bar 5) are copied and pasted with Kick selected and the cursor at bar 17
- **THEN** copies appear on Kick at bar 17 and on Bass at bar 21

#### Scenario: Drag to another track
- **WHEN** the Lead clip "theme" is dragged one bar right and down onto Pad
- **THEN** it moves to Pad one bar later and the Pad clips it covers are trimmed

### Requirement: No overlapping clips
A clip placed by move, resize, paste or duplicate SHALL trim the clips it overlaps on its track: a covered clip is
removed, a partly covered clip is shortened, and a clip covered in its middle is split in two.

#### Scenario: Split by paste
- **WHEN** a 2-bar clip is pasted into the middle of a 16-bar clip
- **THEN** the long clip becomes two clips around the pasted one

### Requirement: Align to grid
Q (or the Align button) SHALL move the start of each selected clip, or of every clip when none is selected, to the
nearest snap position (Bar when snap is Off), then resolve overlaps.

#### Scenario: Align after a nudge
- **WHEN** a clip at bar 13.25 is aligned with snap Bar
- **THEN** it starts at bar 13

### Requirement: Knob values persist
A knob change SHALL be stored in the track's patch or effect at once, so re-selecting the track, undo, or adding,
removing and moving slots or effects shows the stored value. Changing an instrument knob SHALL mark the preset edited
("acid-bass*") in the device header and the track header; loading a preset SHALL replace the values. An envelope's
curve SHALL redraw while its knobs change.

#### Scenario: Value survives re-selection
- **WHEN** the user sets Bass Glide to 1.37 s, selects Lead, then Bass again
- **THEN** Glide still shows 1.37 s and the preset reads "acid-bass*"

### Requirement: Knobs follow automation
A knob whose parameter has an automation lane SHALL be marked automated and SHALL show the lane's value at the
playhead: while playing, on seek, and when a lane point is edited. Turning an automated knob by hand SHALL override
that lane (the knob and the lane value are shown struck/dashed) until "Re-enable automation" is clicked in the device
panel's context line. Override is editor state and does not change the exported song.

#### Scenario: Cutoff follows the lane
- **WHEN** Bass has a filt1.cutoff lane and the user seeks to bar 3 and plays
- **THEN** the Filter 1 Cutoff knob reads the lane value (e.g. 1.27k at bar 3) and moves with it during playback

#### Scenario: Override and re-enable
- **WHEN** the user drags the automated Reso knob while playing
- **THEN** the knob stops following the lane, "↺ Re-enable automation (1)" appears, and clicking it returns the knob
  to the lane value

### Requirement: Automation lanes
A track SHALL hold any number of automation lanes, at most one per (device, parameter), each targeting any parameter
of any device in the track's chain. The track header SHALL show the lane count and a show/hide toggle; each lane SHALL
have a target picker (already-automated parameters disabled), its value at the playhead and a remove button, and the
track SHALL offer "+ lane".

#### Scenario: Draw automation
- **WHEN** two points are drawn on the cutoff lane at bar 1 (0.2) and bar 3 (0.8)
- **THEN** the cutoff rises linearly between them during playback

#### Scenario: Several lanes on one track
- **WHEN** the bass track has lanes for Unisynth `filt1.cutoff`, Unisynth `filt1.resonance` and Crusher `dry_wet`
- **THEN** all three play back together, and the picker of a new lane offers none of those three parameters

### Requirement: Automation point editing
Clicking an empty spot in a lane SHALL add a point there. Dragging a point SHALL move it in time (snapped to the snap
setting, Alt for free) and value, never past its neighbours, with a tooltip of position and physical value. Each
gesture SHALL be one undo step.

#### Scenario: Move a point
- **WHEN** the cutoff point at bar 12 is dragged right and down with snap 1/16
- **THEN** it lands on a 1/16 position before the next point, the tooltip shows e.g. "12.2.480 · 3.07k", and playback
  follows the new curve

### Requirement: Automation point removal and keyboard
A double-click, right-click or Delete/Backspace on a point SHALL remove it; a lane keeps at least one point. A focused
point SHALL move with arrow keys (Shift for fine steps) and announce its value and position.

#### Scenario: Delete a point
- **WHEN** a point is double-clicked, right-clicked, or focused and Delete is pressed
- **THEN** the point is removed, the neighbours are joined, and Ctrl+Z restores it

#### Scenario: Last point
- **WHEN** a lane has one point and the user deletes it
- **THEN** the point stays and the DAW says to remove the lane instead

### Requirement: Track selection
Exactly one track (including a bus or the master) SHALL be selected. Clicking its header, a clip on it or its mixer
strip name, or pressing Enter on a focused header, SHALL select it; Arrow Up/Down on a focused header SHALL select the
neighbour. Selecting SHALL highlight the header, lane and strip, and SHALL rebuild the device panel for that track's
chain (instrument or bus note, then effects), the piano roll for its current clip, and the panel context line.

#### Scenario: Select a bus
- **WHEN** the user clicks the "Verb bus" header
- **THEN** the device panel shows "no instrument, fed by sends from Lead, Pad, Keys, Hats" followed by its Reverb and EQ

#### Scenario: Clip selects its track
- **WHEN** the user clicks a Lead clip while Bass is selected
- **THEN** Lead becomes selected and the piano roll shows that clip in Lead's colour

### Requirement: Piano roll
Opening a clip SHALL show a piano roll with a keyboard column, a note grid snapped to the current snap, and a velocity
lane. The user SHALL be able to draw, select (click, box, Ctrl+A), move, resize, delete, transpose (arrow keys,
Shift for octave), change velocity, and quantize notes. Clicking a key SHALL audition it on the clip's track.

#### Scenario: Draw a note
- **WHEN** the user draws on the grid at C3 at beat 2 with snap 1/16
- **THEN** a note C3 (MIDI 48) starts at tick 960 with length 240 and is heard on the next pass

### Requirement: Clip notes and roll length
Notes SHALL belong to the clip: linked placements share one note list, and a placement shorter than its notes plays
their start. Clicking a clip SHALL open it in the piano roll, which spans exactly the clip (bars × 16 steps) and shows
its name, position and link count. Copied, pasted or moved clips SHALL carry their notes.

#### Scenario: Roll matches the clip
- **WHEN** the 8-bar clip bass-c is clicked
- **THEN** the piano roll is 8 bars (128 steps) wide and shows "bass-c · bar 21 · 8 bars"

### Requirement: Clip thumbnails and roll markers
The arrangement thumbnail of a clip SHALL draw the clip's own notes and update after every edit. The piano roll SHALL
draw the song's loop range and playhead at their positions inside the open clip, and omit them outside it.

#### Scenario: Linked edit
- **WHEN** all notes of piano-a are deleted in the roll
- **THEN** both linked piano-a thumbnails are empty and piano-b is unchanged

### Requirement: Note selection
In the piano roll, click SHALL select a note, Shift-click SHALL toggle it, dragging on empty grid SHALL select the notes
the rectangle touches, Ctrl+A SHALL select all and Esc SHALL clear. A single click on empty grid SHALL set the insert
cursor (snapped). The toolbar SHALL show the selection count, pitch and velocity.

#### Scenario: Marquee
- **WHEN** the user drags over bar 1 across all pitches
- **THEN** every note starting or sounding in bar 1 is selected

### Requirement: Note editing
Selected notes SHALL move by drag (time snapped, pitch by row) or Arrow Left/Right (snap, Shift = 1 bar), change length
by dragging the right edge or Alt+Arrow Left/Right, transpose by Arrow Up/Down (Shift = octave) or the −12/−1/+1/+12
buttons, and be cut, copied, pasted at the insert cursor, duplicated after the selection (Ctrl+D) and deleted.

#### Scenario: Copy a bar
- **WHEN** bar 1's notes are copied and pasted with the cursor at 3.1.0
- **THEN** the same pitches, lengths and velocities appear from bar 3, selected

#### Scenario: Transpose scrolls into view
- **WHEN** selected notes are transposed an octave up past the visible rows
- **THEN** they move 12 semitones and the roll scrolls so they are visible

### Requirement: Proportional note resize
Dragging the right edge of one of several selected notes SHALL scale every selected length by the grabbed note's
factor (the grabbed note snaps, the others round to the tick); with Alt held every selected note SHALL change by the
same amount.

#### Scenario: Proportional resize
- **WHEN** notes of 4 and 2 steps are selected and the 4-step note is dragged to 3 steps
- **THEN** the notes are 3 and 1.5 steps long, and one Undo restores both

### Requirement: Adding and deleting notes
Double-click on empty grid SHALL add a note (snap length, last velocity); double-click on a note SHALL delete it. Notes
SHALL stay inside the clip and the MIDI range.

#### Scenario: Add by double-click
- **WHEN** the user double-clicks empty grid at G6, step 20, with snap 1/16
- **THEN** a 1/16 note G6 is added at 2.2.0 with the last used velocity and selected

### Requirement: Quantize and legato
Quantize (Q, or the button) SHALL move note starts of the selection, or of all notes when none is selected, toward the
snap grid by a strength of 100, 75 or 50 %; Shift+Q SHALL also quantize lengths. Legato SHALL extend each note to the
start of the next.

#### Scenario: Half-strength quantize
- **WHEN** a note at step 4.3 is quantized to 1/16 at 50 %
- **THEN** it starts at step 4.15

### Requirement: Piano roll pitch scrolling
The piano roll SHALL scroll vertically over the MIDI range with the mouse wheel or scrollbar (Shift+wheel scrolls in
time), with the key column following, and SHALL open centred on the clip's notes.

#### Scenario: Open centred
- **WHEN** a bass clip with notes around C2 is opened
- **THEN** the visible rows include C2 without scrolling

### Requirement: Clipboard and undo
Notes and clips SHALL each have their own clipboard, so copying notes does not replace copied clips. Every edit command
or drag gesture SHALL be one undo step (Ctrl+Z, Ctrl+Shift+Z, and the toolbar buttons), and the status line SHALL say
what happened.

#### Scenario: Undo a paste
- **WHEN** ten notes are pasted and the user presses Ctrl+Z
- **THEN** the ten notes disappear and the previous selection returns

### Requirement: Velocity editing
The piano roll SHALL let the user set note velocity (1–127) in the controller lane, one stem per note; notes starting
together get stems side by side (lowest pitch first) and hovering a stem outlines its note. Dragging a stem SHALL set
that note only; on a selected note with several selected it SHALL shift the selection by the same amount (Alt: all
equal, Ctrl/⌘: that note only). Dragging from an empty spot SHALL paint the stems the sweep crosses. Each gesture is
one undo step.

#### Scenario: Paint velocities
- **WHEN** the user drags upward over the first note's stem
- **THEN** its velocity rises, the lane shows e.g. "vel 103", and the note plays louder on the next pass

#### Scenario: Selected notes together
- **WHEN** three notes are selected and one of their stems is dragged down by 20
- **THEN** all three drop by 20 and stay within 1–127

#### Scenario: One note of a chord
- **WHEN** the user drags the three stems of a G3–B3–D4 chord to 30, 70 and 110 in turn
- **THEN** the notes have velocities 30, 70 and 110 and the other chords are unchanged

#### Scenario: One note inside a selection
- **WHEN** all notes are selected and the user Ctrl-drags the B3 stem of the second chord
- **THEN** only that B3 changes

### Requirement: Velocity nudge and field
The mouse wheel over a stem SHALL nudge velocity by 1 (Shift by 8), applied to the selection when the stem is selected.
A Vel field in the toolbar SHALL show the selected velocity ("mixed" when they differ) and set every selected note to
the typed value.

#### Scenario: Velocity field
- **WHEN** 24 notes with velocities 53–112 are selected and 64 is typed into Vel
- **THEN** all 24 notes have velocity 64 and the toolbar shows "vel 64"

### Requirement: Mod wheel and pitch bend lanes
The piano roll's controller lane SHALL also show the clip's mod-wheel (0–127) and pitch-bend (± the instrument's bend
range, centre line shown) envelopes, edited like automation points, with bend snapping to centre near it. They belong
to the clip, so linked placements repeat them.

#### Scenario: Wheel opens the filter
- **WHEN** a mod-matrix slot routes Mod wheel to `filt1.cutoff` and the clip's wheel lane rises from 0 to 100
- **THEN** the cutoff opens over that span in every placement of the clip

### Requirement: Audition controllers
The DAW SHALL offer on-screen mod-wheel and pitch-bend controls (bend springs back to centre) and SHALL pass Web MIDI
wheel and bend to the armed track. Audition controllers SHALL NOT be recorded.

#### Scenario: Bend springs back
- **WHEN** the user releases the on-screen bend control
- **THEN** it returns to centre and the armed instrument's pitch returns

### Requirement: Device panel
Selecting a track SHALL show its device chain. Each device editor SHALL be generated from the host's parameter tables,
grouped as the tables group them, each control showing the physical value and unit, choice parameters as selects,
double-click resetting to default and Shift for fine steps. Changing a structural parameter SHALL rebuild the device.

#### Scenario: Generated controls
- **WHEN** a kind-64 instrument is shown
- **THEN** every one of its parameters has exactly one control, and editing a control changes the sound while playing

### Requirement: Slot racks
The instrument editor SHALL show its slot groups (oscillators, filters, envelopes, LFOs, mod slots) as blocks in a
fixed layout: every slot always at its own position, with a used/maximum count per block. An unset slot (type Off, or
all values at default) SHALL start collapsed to its header and type control; a set slot SHALL start expanded. Setting
its type SHALL expand it without moving any other slot. The mod matrix SHALL be a table of 12 rows (source, target,
operation, amount).

#### Scenario: Turning on filter 3
- **WHEN** an instrument uses filter 1 only and the user sets filter 3's type in its collapsed header
- **THEN** filter 3 expands at the third position, filter 2 stays collapsed and Off, and the count reads 2/4

#### Scenario: Stable positions
- **WHEN** the user turns oscillator 2 on and off again
- **THEN** no other slot changes position or number and nothing is renumbered in the song

#### Scenario: Block markings
- **WHEN** the instrument editor shows a patch
- **THEN** Envelope 1 (amplitude) is marked fixed, the filter block shows the serial order, and mod rows whose source is None are dimmed

### Requirement: Slot remove and move
Clearing a slot (✕) SHALL reset that slot to off or defaults in place; other slots SHALL NOT move. Clearing SHALL
clear the mod slots that targeted the cleared slot and remove its automation lanes. Only filters SHALL be reordered: a
handle between two neighbouring filters SHALL swap their values. Each SHALL be one undo step; a swap SHALL rewrite the
mod-matrix targets and automation lanes of both filters.

#### Scenario: Clear in place
- **WHEN** oscillators 1 and 2 are on, mod slot 1 targets `osc2.gain`, and the user clears oscillator 1
- **THEN** oscillator 1 is Off at its position, oscillator 2 keeps its number and values, and mod slot 1 still targets
  `osc2.gain`

#### Scenario: Reorder keeps modulation
- **WHEN** env 2 modulates `filt1.cutoff`, a lane automates `filt1.resonance`, and filters 1 and 2 are swapped
- **THEN** the mod slot targets `filt2.cutoff`, the lane targets `filt2.resonance`, and the sound is unchanged except for
  the new filter order

### Requirement: Device chain and presets
The device panel SHALL load and save presets, add, remove and reorder effects of the kinds the build supports, and
bypass a device.

#### Scenario: Load preset
- **WHEN** preset `acid-bass` is loaded on the bass instrument
- **THEN** every control shows the preset values and the change is one undo step

### Requirement: Effect order and automation
Effects SHALL move earlier or later in the chain (◀ ▶ on the device, or the insert list); automation lanes of moved
effects SHALL follow them. Removing an effect SHALL remove its lanes and say how many. Bypass (⏻) SHALL be one state
shown in the device panel and on the mixer insert.

#### Scenario: Move with its lane
- **WHEN** Bass has a lane on Crusher mix and Crusher is moved after Compressor
- **THEN** the chain reads "… → Compressor → Crusher" and the lane still targets Crusher · mix

### Requirement: Instrument library
The DAW SHALL show one searchable library of every instrument source the server finds: project presets (`us-patch/1`),
the GM bank (instruments with bank, program, kit flag and region count), Tunefish programs, instruments inside
WaveSabre songs, and the repository test patches, each with a count, a licence note and how it converts.

#### Scenario: Search across sources
- **WHEN** the user types "strings"
- **THEN** matches from every source are listed with per-source counts (for example GM 5 of 235, Tunefish 3 of 92)

### Requirement: Loading from the library
Selecting a library entry SHALL show its details with Audition and "Load into <selected track>". Loading a Tunefish or
WaveSabre entry SHALL use its ahead-of-time `synth-convert` conversion served with the catalog: the converted
instrument replaces the track's and a Tunefish program's insert effects follow it; loading a GM entry SHALL set a
sampler slot to that instrument. Each load is one undo step; sources outside the project are read-only.

#### Scenario: Load a converted Tunefish program
- **WHEN** a Tunefish program whose conversion is an instrument plus one insert effect is loaded into the Lead track
- **THEN** the Lead's instrument is replaced by the converted one, the insert follows it before the Lead's own effects,
  lanes of the Lead's later effects keep their targets, and one undo restores the Lead

#### Scenario: Entry not converted yet
- **WHEN** the converted-library file has no entry for a Tunefish program
- **THEN** "Load into" is disabled and the inspector says how to convert the library

#### Scenario: Load a GM instrument
- **WHEN** "Piano 1d" (bank 16, program 0) is loaded into the Keys track
- **THEN** a sampler slot uses that instrument, the key map shows its 10 regions, and the preset name reads "GM Piano 1d"

### Requirement: GM sampler editor
A sampler slot SHALL offer the sources "GSM sample (in chunk)", "GM.DLS wave" and "GM.DLS instrument", a GM picker that
opens the library on the GM bank, and a key map of the regions that will play (one region over all keys for a single
wave). The DAW SHALL say when the bank is missing and when the song depends on an OS bank that differs per platform.

#### Scenario: Wave vs instrument
- **WHEN** the source of a slot holding "Piano 1" changes from "GM.DLS instrument" to "GM.DLS wave"
- **THEN** the key map changes from 10 regions to one wave over keys 0–127

### Requirement: Mixer
The DAW SHALL show a mixer with a strip per track and the master: volume in dB, mute, solo, sends and live peak
meters. Solo SHALL mute every track that is not soloed, not a send source of a soloed
track, and not a bus a soloed track sends to. Mute and solo are editor state; only mute is saved and exported.

#### Scenario: Solo keeps the reverb bus
- **WHEN** Lead, which sends to Verb bus, is soloed
- **THEN** Lead and Verb bus play and all other tracks are silent

#### Scenario: Solo keeps sidechain source
- **WHEN** a pad track that receives the kick on its sidechain is soloed
- **THEN** the kick still feeds the sidechain and the pad ducks, while other tracks are silent

### Requirement: Insert effects on every strip
Each mixer strip, including bus tracks and the master, SHALL list the track's effect chain as inserts in signal order,
with bypass per insert, "+ insert" offering the effect kinds the build supports, and opening an insert in the device
panel. Inserts and the device panel's chain SHALL be the same data.

#### Scenario: Master insert
- **WHEN** a compressor is inserted on the master
- **THEN** it processes the summed song output, appears in the master's device chain, and is exported as a device on
  the master track

### Requirement: Sends and bus tracks
A strip SHALL show its sends with target, channel (main or sidechain) and level, and "+ send" to any track that does
not feed it. Sends SHALL be post-fader and SHALL be stored as receives on the target track. "+ Bus" SHALL add a track
with no instrument that is fed only by sends; bus tracks SHALL show in the arrangement without clips.

#### Scenario: Shared reverb bus
- **WHEN** Lead and Pad send to bus "Verb bus" (Reverb insert) at −10 dB and −4 dB
- **THEN** the bus track receives both at those levels after their faders and its output reaches the master

#### Scenario: No feedback loop
- **WHEN** the user opens "+ send" on the Verb bus strip
- **THEN** Lead and Pad are not offered as targets, because they feed the bus

### Requirement: Computer keyboard and MIDI audition
The computer keyboard SHALL play the armed track like a tracker's note keyboard (FastTracker 2 / Renoise layout, by
physical key), with keys for octave and velocity. Selecting an instrument track SHALL arm it; the ● button arms
another track by hand. Web MIDI input SHALL play the armed track when the browser allows it. Audition SHALL NOT
record notes.

#### Scenario: Keyboard note
- **WHEN** the user selects the lead track and presses `Z`
- **THEN** the lead instrument plays C of the current octave until the key is released, and `Q` plays the C above

#### Scenario: Tracker layout
- **WHEN** the user plays Z S X D C V G B H N J M, then Q 2 W 3 E R 5 T 6 Y 7 U I 9 O 0 P
- **THEN** the first row plays the twelve notes from C and the second the next octave and a half, on any keyboard layout

#### Scenario: Octave and velocity
- **WHEN** the user presses ] (or PageUp, numpad *) and then ' (or numpad +)
- **THEN** the next notes sound an octave higher and louder; [ and ; go the other way

### Requirement: Computer keyboard mode
While the computer keyboard is on, its note keys SHALL take precedence over letter shortcuts that share a key (Q
quantize stays on Ctrl+Q or Shift+Q); F (Follow), L (loop) and Shift+R (record) are not note keys. ` or the ⌨ button
SHALL switch the computer keyboard off so the letters are shortcuts again. Pressed notes SHALL light their keys in the
piano roll. Typing in a text field never plays notes.

#### Scenario: Q plays or quantizes
- **WHEN** the keyboard is on and the user presses Q, then presses ` and Q in the piano roll
- **THEN** the first Q plays C on the selected track and the second quantizes the selected notes

### Requirement: Automation record
A Record button (●, Shift+R) SHALL arm automation recording. While recording and playing, turning or holding a device
knob SHALL write its value into that parameter's automation lane at the playhead (1/64-bar grid), creating and showing
the lane when needed. The knob SHALL keep following its lane, each gesture SHALL be one undo step, and the engine SHALL
hear the recorded lane while recording. Stopped, a knob turn is an ordinary edit.

#### Scenario: Record a filter sweep
- **WHEN** record is on, the song plays and the user drags the Lead cutoff knob up for two beats
- **THEN** Lead gets a filt1.cutoff lane rising over those beats, shown in the track list, and one Undo removes it

#### Scenario: Touch overwrite
- **WHEN** a lane already has points and the user holds its knob still while the song plays over them
- **THEN** the points passed while holding are replaced by the held value; points before the touch and after the release stay

### Requirement: Undo and redo
Every edit to the song SHALL be undoable and redoable (Ctrl+Z, Ctrl+Shift+Z), including parameter changes, where one
drag gesture is one undo step.

#### Scenario: Undo a knob drag
- **WHEN** a cutoff knob is dragged through many values and the user presses Ctrl+Z
- **THEN** the cutoff returns to its value before the drag

### Requirement: Offline bounce
The DAW SHALL render the whole song or the loop range to a WAV file in the background through the player on the
exported blob, show progress, and let the user cancel. The bounced audio SHALL equal `song render` output for the same
song.

#### Scenario: Bounce matches CLI
- **WHEN** a song is bounced in the DAW and rendered with `song render`
- **THEN** the two WAV files are identical

### Requirement: Accessibility and layout
All controls SHALL be reachable and operable by keyboard with a visible focus, have accessible names, and meet WCAG
2.2 AA contrast. Panels SHALL be resizable with splitters that remember their size. The DAW targets desktop widths
from 1280 px.

#### Scenario: Keyboard knob
- **WHEN** a knob has focus and the user presses Arrow Up
- **THEN** the value rises by one step and the change is announced with its physical value

### Requirement: GSM samples
The library SHALL list the GSM samples stored in the instruments of project songs (once per distinct sample) with
length and size; one SHALL play on its own and load into the selected instrument's sampler slot. The DAW SHALL import
an audio file as a GSM sample (resampled to 44.1 kHz mono, GSM 06.10 bit-exact with libgsm, at most 30 s), reporting
its size, and SHALL remove stored samples no slot plays on request.

#### Scenario: Import a WAV
- **WHEN** the user picks Import sample… on the Lead and chooses a 0.5 s stereo 48 kHz WAV
- **THEN** the Lead stores one GSM sample (about 4.5 KB), a sampler slot plays it, and the status reports its size

#### Scenario: Reuse a sample from another song
- **WHEN** the user plays a GSM sample entry in the library and loads it into the selected track
- **THEN** the sample is copied into that instrument (not duplicated if already stored) and a sampler slot plays it

### Requirement: Live device graphics
Envelope curves SHALL be drawn as the engine plays them (per envelope mode) and redraw when a parameter they depend
on changes, by hand or by automation. A sampler slot SHALL show its sample's waveform with start and loop, and its
playback settings (loop mode, region, interpolation, reverse) as named controls. Every active oscillator SHALL show its
waveform, every active filter its response with the cutoff marked, and every LFO its shape, redrawn on change.

#### Scenario: Turning release
- **WHEN** the user turns Env 1 release
- **THEN** the envelope curve changes while the knob moves

#### Scenario: Setting a loop on a sample
- **WHEN** the user sets loop to Repeat, region to Custom and drags the loop end on the waveform
- **THEN** loop start and length knobs appear and follow the drag, and Undo restores the previous loop

#### Scenario: Loop knobs only for a playing custom loop
- **WHEN** a sampler slot's loop is off, or its region is the whole sample
- **THEN** no loop start or length knobs are shown; dragging the start line or the loop edits those values in one undo step

#### Scenario: Turning cutoff
- **WHEN** the user turns filter 1 cutoff
- **THEN** the response curve and its cutoff mark move while the knob moves

### Requirement: Collapsible blocks and slots
Every slot and every block of the instrument editor SHALL have a collapse/expand toggle exposed as a button with
`aria-expanded`. A block's collapsed state SHALL persist across track changes and reloads in browser storage and SHALL
NOT be saved in the song.

#### Scenario: Collapse survives a track change
- **WHEN** the user collapses the Mod matrix block, selects another track and selects the first again
- **THEN** the Mod matrix block is still collapsed and the saved song is unchanged

### Requirement: Block colours and routing hints
Each block (voice, oscillators, filters, envelopes, LFOs, mod matrix) SHALL have its own accent colour on set slots;
slots that are not set SHALL show no accent. Envelope and LFO slots SHALL show the targets that mod rows route them to,
or "unrouted" with their knobs dimmed. Mod rows SHALL mark their source's block colour. Text SHALL meet WCAG AA contrast.

#### Scenario: Routing hint
- **WHEN** mod slot 2 routes LFO 1 to `filt1.cutoff`
- **THEN** the LFO 1 header shows "→ filt1.cutoff" and mod row 2 carries the LFO block colour

### Requirement: Live per-voice displays
While the selected track's instrument sounds (playback, keyboard or MIDI audition), its screens SHALL show the
engine's voice state with one marker per voice: sampler playheads, envelope level dots and level bars, LFO phase dots.
Each voice SHALL keep one colour in every block and a note chip in the device header. Marker brightness SHALL follow the
voice's amplitude; released voices SHALL be shown faded.

#### Scenario: A chord on the sampler
- **WHEN** a four-note chord plays on a track whose oscillator 1 is a sampler
- **THEN** four chips show the notes, the waveform shows four playheads in those colours and each envelope shows four dots

#### Scenario: Silence
- **WHEN** all voices have ended
- **THEN** no markers or chips are shown and the screens show the static curves

### Requirement: Coinciding and excess voice markers
Markers of voices at the same position SHALL stack so each stays visible. At most 16 voices SHALL be drawn, the loudest;
beyond that the chip row SHALL show "+N".

#### Scenario: Chord at sustain
- **WHEN** four voices hold at the sustain level of envelope 1
- **THEN** four distinct dots are visible at the sustain point

#### Scenario: Many voices
- **WHEN** 24 voices sound
- **THEN** 16 voices are drawn and the chip row ends with "+8"

### Requirement: Live filter spectrum
While voices sound, each active filter screen SHALL show the spectrum of the filter's input in grey and of its output
in the filter block's colour, summed over the voices, and a tick per voice at that voice's modulated cutoff.

#### Scenario: Low-pass at work
- **WHEN** a bright saw chord plays through a low-pass filter at 300 Hz
- **THEN** the grey input shows harmonics above 300 Hz that the coloured output does not

#### Scenario: Modulated cutoff
- **WHEN** an LFO modulates filter 1's cutoff during a chord
- **THEN** the cutoff ticks of the voices move over time, apart from each other when their LFO phases differ

### Requirement: Live view toggle
A Live toggle in the instrument header SHALL turn the live per-voice view on and off. It is on by default and remembered in the browser, not in the song. While it is off, the editor watches no track, so the engine does no probe work.

#### Scenario: Live view off
- **WHEN** the user turns Live off in the instrument header and plays notes
- **THEN** the editor watches no track, status reports carry no probe data, no voice chips or markers are drawn, and the choice is kept after a reload

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

### Requirement: Points and PAD table oscillator blocks
The device panel SHALL show a Points slot's points as one column per point (offset, value, the mode of the segment
ending at it) plus the last segment's mode, following the slot's point count, with a screen drawing the cycle the
engine builds. A PAD table slot SHALL show bandwidth, scale, damp, harmonics, profile (by name) and drive, with a
screen sketching its partials.

#### Scenario: Point count
- **WHEN** a Points slot's point count is set to 4
- **THEN** the slot shows four point columns and the last-segment column, and its screen marks four points

#### Scenario: Cycle drawing
- **WHEN** the panel draws a Points slot
- **THEN** the drawn cycle equals the engine's `pts::cycle` for the same values within 1e-4

#### Scenario: PAD profile name
- **WHEN** a PAD table slot's profile knob is at its maximum
- **THEN** its value text is "gauss"

### Requirement: PAD table inputs rebuild the device
A PAD slot's table inputs (bandwidth, scale, damp, harmonics, profile) SHALL take effect by rebuilding the device, as
structural values do, because the table is built when the instrument loads. They SHALL NOT be automation targets.
Drive SHALL stay a live value.

#### Scenario: Live host
- **WHEN** a song update changes a PAD slot's bandwidth
- **THEN** the live host rebuilds that track's device; a change of the slot's drive is a parameter set

#### Scenario: Knob drag
- **WHEN** the user drags a PAD table input knob
- **THEN** the DAW sends the song to the engine once, when the knob is released, and the parameter is not offered as an automation lane

### Requirement: Tunefish 3 presets source
The instrument library SHALL show "Tunefish 3 presets" after the Tunefish 4 programs, tagged by program number, and
load a converted preset like a converted Tunefish 4 program (instrument plus its insert effects).

#### Scenario: Listed and loadable
- **WHEN** the catalogue holds converted Tunefish 3 presets
- **THEN** the source lists them as `#n` entries that load into the selected track

### Requirement: Effect modes by stored position
The DAW SHALL decode an effect's mode through the engine's stored positions: each effect table's mode parameter
defaults to the mode's stored value, and its options list every position, with "—" at a free one. A device whose
stored value selects a mode SHALL show that mode's table. Adding an effect SHALL write its mode's stored value. The
mode selector SHALL never offer a free position.

#### Scenario: Expanded distortion
- **WHEN** a song holds a distortion device whose mode value is 5/6 (knee, position 5 of 7)
- **THEN** its panel shows "Distortion · Knee" with the knee's threshold, ratio and offset

#### Scenario: Earlier value
- **WHEN** a song written before add-elevated-import holds a distortion with mode value 1/3 (curve of four modes)
- **THEN** the panel shows "Distortion · Curve"

### Requirement: Elevated fields in the device panel
A Pair slot SHALL show its wave, phase shift, detune and polarity, with a drawing of its cycle. The voice SHALL show
the pitch drop's base (Hz) and rate (as the time to fall halfway, "off" at 0). Each envelope card SHALL show `linear`
and `hold`, and a linear envelope's attack, decay and release SHALL read in its own law (`5000·p²` ms, no 1 ms floor).

#### Scenario: Converted Atrium
- **WHEN** the converted Atrium opens on the Device tab
- **THEN** Osc 1 shows "Pair" and its four controls, Env 1 shows linear "On" and its hold, and the first track's effects
  read Knee, Sine, SVF LFO and Ping-pong

### Requirement: Oscillator fields follow the type
An oscillator slot's panel SHALL show the fields its type reads: its shape labels, its common fields, and value text
in the type's units. A type change SHALL re-render the slot with the new type's fields. Switching a slot to an
engine type (Shaper tone, Sweep drum) SHALL write that type's default shape values in the same undo step as the type
change. Other types SHALL keep today's fields and values.

#### Scenario: Switch to a drum
- **WHEN** a slot's type changes from Phase mod to Sweep drum
- **THEN** the slot shows volume, start frequency, sweep, growth, drive, shape, envelope start, slope and curve with
  the drum's defaults, plus only coarse, fine, octave, ratio, gain, mix and pan of the common fields, and one undo
  restores the Phase mod slot with its earlier values

#### Scenario: Back to another type
- **WHEN** a Sweep drum slot changes to Pair
- **THEN** the slot shows Pair's four fields and the common fields as before

### Requirement: Engine type fields
A Shaper tone slot SHALL show the following fields, each in its unit, and a drawing of its first generated frames
from the engine's frame function:
- carrier and modulator (periods), depth, volume
- drive, shape, FM, rate

A Sweep drum slot SHALL show its nine fields and a drawing of its envelope-shaped output over its length.

#### Scenario: Converted Fudge
- **WHEN** the converted Alien Sex Clone opens on the Device tab with instrument 8's track selected
- **THEN** Osc 1 shows "Sweep drum", volume 257, start frequency 0.11 rad, curve "1 − sin" and a falling drawing
