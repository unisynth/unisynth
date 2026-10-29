# Spec Delta

## Purpose
An editable, git-friendly song source (`song/1`) and patch preset (`us-patch/1`) for the unified synth, which export
to and import from the `synth-mix` song blob that the intro player reads.

## ADDED Requirements

### Requirement: Song source file
A song SHALL be one JSON file `song/1` holding: format tag, sample rate, tempo (BPM), time signature (display only),
ticks per beat (960), song length in ticks, clips, tracks and a master track. A track SHALL hold an id, name, colour,
volume, mute, receives (sending track id, channel `main` or `side`, volume), a device chain, clip placements and
automation lanes.

#### Scenario: Minimal song
- **WHEN** a file has the format tag, rate, tempo, one empty instrument track and a master
- **THEN** it validates and exports a blob with two tracks and no events

### Requirement: Song metadata
A song MAY carry a `meta` object with author, description, copyright and url (all required, not blank, the url `http`
or `https`) and optionally title, license (SPDX) and `adapted_from`. A song with `adapted_from` SHALL say in its
description that it is adapted. Metadata SHALL be kept by load and save and SHALL NOT be part of the blob.

#### Scenario: Adapted work
- **WHEN** a song has `adapted_from` and a description that does not contain "adapted"
- **THEN** validation fails at `/meta/description`

#### Scenario: Metadata is not exported
- **WHEN** the same song is exported with and without `meta`
- **THEN** the two blobs are byte-identical

#### Scenario: Import with authorship
- **WHEN** `song import blob -o song.json --meta meta.json` runs with a valid meta file
- **THEN** the written song carries it and `song info` prints the author, the original and the description; an invalid meta file prints its issues and writes nothing

### Requirement: Devices, clips, placements and lanes
A device SHALL hold a kind and its parameter values keyed by name (normalized 0..1; sample data as base64). A clip
SHALL hold a length and notes (start tick, length ticks, MIDI pitch 0–127, velocity 1–127). A placement SHALL
reference a clip by id with a start tick and an optional loop length, so one clip can be placed many times. An
automation lane SHALL target a device index in the track and a parameter name, and hold points (tick, value 0..1).

#### Scenario: Linked placements
- **WHEN** clip `bass-a` is placed at bar 1 and bar 5 and one of its notes is moved
- **THEN** the moved note sounds in both placements

#### Scenario: Save round trip
- **WHEN** a song is loaded and saved without edits
- **THEN** the file is the same JSON value

### Requirement: Validation with paths
Loading SHALL validate the song against `schema/song.schema.json` and the device parameter tables and report every
error with its JSON path and reason: unknown device kind, unknown parameter name, value out of range, unknown clip or
track id, a receive cycle, a duplicate automation lane, a note or point outside the song, non-finite numbers.

#### Scenario: Duplicate automation lane
- **WHEN** a track has two lanes for device 0 `filt1.cutoff`
- **THEN** validation fails at the second lane's path with "parameter already automated on this track"

#### Scenario: Unknown parameter
- **WHEN** a device on track `lead` sets `osc1.cutof`
- **THEN** validation fails with the path `/tracks/1/devices/0/values/osc1.cutof` and "unknown parameter"

### Requirement: Export to the song blob
Export SHALL write a `synth-mix` blob that `synth_core::mix::Song::parse` accepts, with tempo and rate copied, one
device record per track device carrying the chunk its kind defines (kind 64: unisynth chunk v1, canonical), one MIDI
lane per track with all placements flattened, and tracks in song order with the master last. Muted tracks export
with volume 0 and without events. Export SHALL be deterministic: the same song gives the same bytes.

#### Scenario: Deterministic
- **WHEN** the same song is exported twice
- **THEN** both blobs are byte-identical

### Requirement: Buses and master effects
A track's device chain MAY be empty or hold only effects; such a track is a bus fed by receives. The master MAY hold
effects. A send from track A to track B SHALL be stored and exported as a receive on B (post-fader, channel main or
side).

#### Scenario: Effect-only bus
- **WHEN** a track has no instrument, a Reverb device and receives from two tracks
- **THEN** it validates and exports as a blob track with one device and two receives

### Requirement: Master is the end of the graph
The master SHALL have no sends, and no track SHALL receive from the master; its main output is the song output.
Validation SHALL reject a receive from the master with its JSON path.

#### Scenario: Receive from master
- **WHEN** a track lists a receive from the master
- **THEN** validation fails with "the master cannot send; route through a bus before the master"

### Requirement: Clip controller envelopes
A clip MAY hold mod-wheel and pitch-bend envelopes (points: tick within the clip, value 0..1, bend centre 0.5). Export
SHALL flatten them per placement into the track automation of the instrument's `ctl.wheel` and `ctl.bend` parameters;
outside placements the value returns to its default (wheel 0, bend centre) at the placement end. A track that also has
a track-level lane for the same parameter SHALL fail validation.

#### Scenario: Linked wheel
- **WHEN** clip `bass-a` has a wheel envelope and is placed at bars 5 and 9
- **THEN** the exported automation repeats the envelope at both placements and returns to 0 after each

#### Scenario: Bend at rest
- **WHEN** a clip's bend envelope stays at 0.5 (stored as automation byte 128)
- **THEN** the song renders exactly like the same song without the envelope (the engine treats half a step around the
  centre as no bend)

#### Scenario: Looping placement restarts the envelope
- **WHEN** a clip with a bend envelope is placed looping for two clip lengths
- **THEN** the automation repeats the envelope from its start at the second pass and returns to the centre at the end

#### Scenario: Wheel on track and clip
- **WHEN** a track has a track lane for `ctl.wheel` and its clip has a wheel envelope
- **THEN** validation fails with "wheel is automated by both the track and clip bass-a"

### Requirement: Looping placements
A clip SHALL have a length in bars, and a placement SHALL carry `loop` (default false). A looping placement repeats
the clip's notes every clip length until the placement ends; a non-looping placement plays the clip once and is cut
at its end. Export SHALL flatten both into the track's lane exactly as they sound.

#### Scenario: Loop flattening
- **WHEN** a 4-bar clip is placed for 6 bars with `loop: true`
- **THEN** the exported lane holds the clip's notes from bar 1 to 4 and again from bar 5, with every note cut at the
  end of bar 6

### Requirement: Time and value conversion
Note and automation times SHALL convert from ticks to samples by rounding to the nearest sample; at one sample
note-offs SHALL come before note-ons. Automation values SHALL convert to the blob's bytes by rounding `value × 255`.

#### Scenario: Tick to sample
- **WHEN** a 120 BPM, 44 100 Hz song has a note at tick 960 (beat 2)
- **THEN** the note-on is at sample 22 050

#### Scenario: Overlapping same-pitch notes
- **WHEN** two placements on one track put the same pitch at overlapping times
- **THEN** export emits note-off before the second note-on at the same sample and the output never has two open notes
  of one pitch

### Requirement: Import from the song blob
Import SHALL read any blob `Song::parse` accepts and whose device kinds have parameter tables, and produce a song that
exports back to identical device chunks, track volumes, receives, device chains and automation bytes. Each lane SHALL
become one clip placed once at tick 0, with note times converted from samples to ticks (rounded). Unsupported kinds
(for example legacy kinds 0–39) SHALL fail with an error naming the kind and pointing to `synth-convert`.

#### Scenario: Blob round trip
- **WHEN** a blob of kind-64 devices is imported and exported again
- **THEN** device chunks, routing, volumes and automation bytes are identical and every note is within half a tick
  of its original sample

#### Scenario: Legacy kind
- **WHEN** a WaveSabre blob with kind 1 (Slaughter) is imported
- **THEN** import fails with "kind 1 is a legacy device; convert the song with synth-convert first"

### Requirement: Patch presets
A preset SHALL be one JSON file `us-patch/1` with a name, a device kind and the values that differ from the kind's
defaults, keyed by parameter name. Loading a preset into a device SHALL set every listed value and reset the others to
their defaults. Saving SHALL write only non-default values, sorted by parameter index.

#### Scenario: Preset round trip
- **WHEN** a device is saved as a preset and the preset is loaded into a fresh device
- **THEN** both devices export the same chunk bytes

### Requirement: Song command-line tool
A `song` CLI SHALL provide `export <song.json> -o <blob>`, `import <blob> -o <song.json>`, `render <song.json|blob>
-o <wav>` (rendered through the song player, 16-bit or 32-bit float) and `info` (tracks, devices, length, required
cargo features). It SHALL print errors with JSON paths and exit non-zero on failure.

#### Scenario: Render
- **WHEN** `song render demo.song.json -o demo.wav` runs
- **THEN** the WAV equals the player's render of the exported blob, sample for sample
