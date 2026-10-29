# synth-convert Specification

## Purpose
Converting legacy WaveSabre song blobs, `.tfm` songs and Tunefish patches into unified songs. This covers parity
checks against the legacy engines and the gate that decides when the legacy engines may be retired.

## Requirements

### Requirement: Host-only converter
The converter SHALL be a std command-line tool, never linked into the player. It SHALL read a `synth-mix` song blob
with legacy kinds (WaveSabre 0–12, Tunefish 32–39) or a `.tfm` file, and write a `synth-mix` song blob that uses only
unified kinds (64–73).

#### Scenario: WaveSabre song
- **WHEN** a WaveSabre song blob with Slaughter, Falcon, Specimen, Echo and Cathedral is converted
- **THEN** the output loads in the unified player with no legacy kind and the same track count, routing and lanes

#### Scenario: Malformed input
- **WHEN** the input is truncated, has an unknown legacy kind, or fails legacy decoding
- **THEN** the converter exits non-zero with a message naming the problem and writes no output

### Requirement: Song structure preserved
Converting a WaveSabre song SHALL keep tempo, rate, length, tracks, receives (main and sidechain), track volumes, note
lanes and device order. Each legacy device SHALL become one unified device in its place.

#### Scenario: Sidechain routing
- **WHEN** a song with a Smasher keyed by a sidechain receive is converted
- **THEN** the converted compressor is keyed by the sidechain and receives from the same track at the same volume

### Requirement: Tunefish song structure
Converting a `.tfm` SHALL give one track per non-empty instrument, with that instrument's insert effects as unified
effect devices after it in slot order, plus a master track. An event at row r SHALL land at the start of the
512-frame block that contains time `r · 60/(tempo·4)`, as the `.tfm` player fires it. Velocity 0 SHALL become a
note-off, and the master SHALL apply the `.tfm` player's output scaling.

#### Scenario: Insert chain
- **WHEN** an instrument with effect slots [reverb, none, delay] is converted
- **THEN** its track holds the instrument, then a Tunefish-mode reverb, then a Tunefish-mode delay

#### Scenario: Loudness of a converted song
- **WHEN** a converted `.tfm` renders through the unified player and the original through the legacy i16 player
- **THEN** their integrated loudness differs by at most 1 dB

### Requirement: Instrument mapping
Each legacy instrument SHALL map to a unified slot configuration through the physical values of its parameters
(frequencies, times, ratios, levels), not by copying normalized values. Legacy routes SHALL become mod-matrix slots:
WaveSabre pitch envelope, mod-envelope cutoff and vibrato as Offset slots, Tunefish matrix slots as Scale slots.

#### Scenario: Slaughter cutoff route
- **WHEN** a Slaughter with filter mod amount 0.75 and its mod envelope is converted
- **THEN** the unified patch has an Offset slot from that envelope to filter 1 cutoff, adding up to +9990 Hz

#### Scenario: Tunefish organ gate
- **WHEN** a Tunefish patch routes no envelope to volume
- **THEN** the unified envelope 1 gates the note (instant attack, sustain 1) and ends it where Tunefish's gate closes: within a block of the key-up when no envelope is read by a mod slot, otherwise when the longest of those envelopes has finished

### Requirement: Tunefish instruments in songs
A `synth-mix` kind 32 device SHALL convert in place to a unified instrument. The insert effects of its patch SHALL
become unified effect devices in the Tunefish modes, appended to the song and listed after the instrument in every
track that lists it, in slot order. Automation of an effect parameter SHALL move to that effect device at the
parameter's index in the effect's own order plus one.

#### Scenario: Effect automation
- **WHEN** an automation of a Tunefish chorus depth drives a kind 32 device whose patch has a chorus in slot 1
- **THEN** the converted song automates the unified chorus device at parameter index 2 (depth is its second parameter, after the mode)

### Requirement: Fixed routing
Automation of an effect slot's kind, or of the source or target of a mod slot, SHALL be an error naming the parameter,
because the unified patch cannot change its routing during a song. Automation of a mod slot's amount SHALL be converted.

#### Scenario: Automated slot kind
- **WHEN** an automation drives the kind of an effect slot or the source of a mod slot
- **THEN** the converter exits non-zero naming that parameter and writes no output

### Requirement: Automation re-mapping
WaveSabre automation of instrument parameters SHALL be re-mapped point by point through the legacy and unified
parameter curves. Points SHALL be added where linear interpolation in the unified curve would differ from the legacy
value by more than one byte step. Effect automation SHALL be copied with the parameter index shifted past the mode
parameter.

#### Scenario: Cutoff automation
- **WHEN** a Slaughter filter-frequency automation ramps from 0 to 1 over 1 s
- **THEN** the converted automation's cutoff in Hz stays within one byte step of the legacy cutoff at every 100-frame step

### Requirement: Discrete parameter automation
Automation of a discrete legacy parameter (filter type, voice mode, loop mode, interpolation, boundary, unison) SHALL
be converted by evaluating the legacy decode at every 100-frame step and emitting a step (two points at the same
frame) wherever the decoded value changes, so the converted song never passes through intermediate values. Automation
of oscillator type, voice capacity and sampler source or index SHALL NOT be emitted.

#### Scenario: Filter type sweep
- **WHEN** a Slaughter filter-type automation ramps 0 → 1 over 1 s
- **THEN** the converted song switches the filter type at the frames where the legacy decode changes and at no other frame

### Requirement: Level compensation
A legacy device whose centre pan is not equal power (Thunder: the sample at full level on both sides; Tunefish: a
linear balance with full level at the centre) SHALL be converted with its gain raised by √2, so that its centred
loudness matches.

#### Scenario: Thunder level
- **WHEN** the Thunder test song is rendered by the legacy crate and, converted, by the unified instrument
- **THEN** their integrated loudness differs by at most 1 dB

### Requirement: Feature list
The converter SHALL print the cargo features (oscillator types, filter types, envelope modes, effect kinds and modes)
that a unified song needs, so a compo build links only those modules. A build with exactly the listed features SHALL load and render the song.

#### Scenario: Minimal features
- **WHEN** a song that uses only Slaughter-equivalent instruments and no effects is analysed
- **THEN** the list holds the saw/pulse, noise, state-variable filter and WaveSabre envelope features only (the instrument itself is always built)

#### Scenario: Automation reach
- **WHEN** a lane steps a filter's type, raises the noise level or switches an envelope's mode
- **THEN** the list holds the modules of every value the lane can reach, and a build with exactly the list loads the song and plays all of them

### Requirement: Device parity
For every legacy device, an opt-in tool SHALL convert that device's legacy synthetic test songs, render the legacy
song with its legacy crate and the converted one with the unified crates, and compare them with the shared metrics: 1
dB loudness, 3 dB per octave band 63 Hz–16 kHz, onsets within 100 frames (WaveSabre) or one 512-frame block (Tunefish).

#### Scenario: Falcon test song
- **WHEN** the Falcon legacy test song is rendered both ways
- **THEN** the parity report passes all three metrics

### Requirement: Reference song parity
The tool SHALL convert and compare the reference songs Global Groove, Elysian, Engage and Eladamri (WaveSabre) and
no_remorse and tf4testsong (Tunefish) with the same metrics and onset tolerances. Songs and renders stay outside the
repo, and the tool SHALL skip with a message when they are absent.

#### Scenario: Elysian
- **WHEN** Elysian is rendered by `crates/wavesabre` and, converted, by `unisynth`
- **THEN** all three metrics are within tolerance

#### Scenario: Songs absent
- **WHEN** the synth lab directory is not present
- **THEN** the tool prints a skip message and exits 0, and the normal test suite does not depend on it

### Requirement: Retirement gate
The legacy crates SHALL NOT be removed until a parity report shows every device and reference song passing and the
user has signed off an A/B listen. The report SHALL be committed (metrics only, no audio).

#### Scenario: Failing song blocks retirement
- **WHEN** any device or reference song fails parity
- **THEN** the report marks retirement as blocked and names the failing item

### Requirement: V2M import
`synth-convert v2m <in.v2m> <out.song> [tail-s]` SHALL read a `.v2m` file of any format version 0–6, detect the version
from its patch and global sizes, and upgrade it to the version 6 layout by inserting V2's defaults for missing
parameters, as V2's own converter does. A file whose version cannot be determined, or that is truncated or malformed,
SHALL be an error that names the problem, writes nothing and exits non-zero.

#### Scenario: Upgrade matches the reference
- **WHEN** each acceptance song is upgraded by the converter and by the reference player's loader
- **THEN** the two version 6 layouts are byte-identical, and the detected versions are equal

#### Scenario: Unknown version
- **WHEN** a file's patch and global sizes match no version
- **THEN** the converter names the sizes it found, exits non-zero and writes no output

### Requirement: V2 song mapping
The converter SHALL map each (MIDI channel, program) pair that plays notes to one track with one unified instrument;
a note SHALL go to the track of the program in force on its channel at its note-on. Note timing SHALL follow V2's tempo
map and tick rate, within one sample. Pitch bend and controllers 1–7 SHALL become the instrument's controller
parameters and automation. Each V2 patch SHALL map to the instrument's V2 modes, with its modulation entries as Add
slots.

#### Scenario: Program change
- **WHEN** channel 1 plays program 3, then a program change to 5 at bar 9
- **THEN** notes before bar 9 play on the track whose instrument was converted from patch 3, and later notes on the track of patch 5

#### Scenario: More modulation entries than slots
- **WHEN** a patch has more modulation entries than the matrix has slots
- **THEN** the converter keeps the entries with the largest absolute amount, prints a warning naming the patch and the dropped entries, and still writes the song

### Requirement: V2 global routing
The converter SHALL make the V2 reverb and delay bus tracks fed by the instruments' send outputs, and aux A/B bus
tracks. It SHALL put the post filters and the sum compressor on the master track. Channel 16 SHALL carry the speech
stage with the song's speech text when the song has one. The source format version SHALL be recorded in each
instrument, so that modes which changed between versions behave as in that version.

#### Scenario: Reverb bus
- **WHEN** a song's patches send to the reverb
- **THEN** the converted song has one reverb bus track receiving every instrument's reverb output pair, and that bus feeds the master

### Requirement: V2 song corpus
A tool SHALL gather V2 songs into a local directory outside the repository: from the farbrausch public sources, the
reference player's corpus and the modland collection, and by carving songs out of packed demo executables. It SHALL
render a reference WAV for each song with the reference player and record per song its source, checksum, format
version and the features it uses. Songs and renders SHALL NOT be committed.

#### Scenario: Carve from a demo
- **WHEN** the tool is given a packed farbrausch intro executable
- **THEN** it writes the embedded song, its checksum matches the provenance list when the song is listed there, and its reference render is written next to it

### Requirement: V2 reference song parity
The parity tool SHALL convert and compare the V2 acceptance songs (fr-08, flybye, fr-014, fr-019, fr-022 final and party,
fr-024, fr-027, fr-029, candytron, brüllwürfel, farbomat, .kkrieger, pzero, v2_zeitmaschine) against the reference
player's renders, with the shared metrics: 1 dB loudness, 3 dB per octave band 63 Hz–16 kHz, onsets within 100
frames. It SHALL skip with a message when the songs are absent, and commit its report (metrics only).

#### Scenario: fr-08
- **WHEN** fr-08 is rendered by the reference player and, converted, by `unisynth`
- **THEN** all three metrics are within tolerance

#### Scenario: Songs absent
- **WHEN** the V2 song directory is not present
- **THEN** the tool prints a skip message and exits 0
