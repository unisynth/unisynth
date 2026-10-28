# Spec Delta

## ADDED Requirements

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
