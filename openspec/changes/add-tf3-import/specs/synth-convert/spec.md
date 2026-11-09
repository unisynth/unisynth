# Spec Delta

## ADDED Requirements

### Requirement: Enigma Studio 3 project import
`synth-convert e3prj <in.e3prj> <out.song> [--song <id>] [--editor] [tail]` SHALL read an Enigma Studio 3 project
(XML), take its Tunefish 3 song (the first non-empty one unless `--song` names one) and the instruments it uses, and
write a `synth-mix` song blob with only unified kinds. A malformed file, an unknown parameter name, a song id that does
not exist or a reference to a missing pattern or instrument SHALL exit non-zero with a message naming it and write
nothing.

#### Scenario: Transplant converts
- **WHEN** Transplant's project is converted
- **THEN** the output loads and renders in `unisynth`, with one track per used instrument plus the master

#### Scenario: Missing pattern
- **WHEN** a pattern instance names a pattern id the project does not hold
- **THEN** the converter names the pattern id and sequence track, exits non-zero and writes no output

### Requirement: Tunefish 3 song structure
Each instrument used SHALL become one track (the instrument, then its insert effects in slot order), with a master
track last. Pattern instances SHALL be expanded on their sequence tracks. Row r SHALL fire at the start of the first 384-frame
block whose start time rounds to row r (`t·bpm/15` rounded half to even, as the player does), one row per block.

#### Scenario: Timing
- **WHEN** a song at 140 bpm has a note at row 16
- **THEN** the note lands at the start of the first block at or after 1.661 s (row 15.5), which is block 191

### Requirement: Tunefish 3 note resolution
Velocity −1 SHALL play 128, and a note with instrument −1 SHALL be ignored, as the player does. A new note on a
column, or note 128, SHALL release that column's sounding note. A note's effect word (slot 1–4, value/128) SHALL become
a controller event on its instrument just before the note (editor mode only).

#### Scenario: Note without instrument
- **WHEN** a column holds a note with instrument 3, then a note with instrument −1
- **THEN** only the first note is played, and it keeps sounding

#### Scenario: Note-off
- **WHEN** a column holds a note and, four rows later, note 128
- **THEN** the note's lane has a note-off at the block of the fourth row

### Requirement: Tunefish 3 instrument mapping
A Tunefish 3 instrument SHALL map through the physical values of its parameters to a unified patch: the point
oscillator to a `points` slot (a second `points` slot one octave down at half gain for the sub-oscillator), the
additive generator to a `pad` slot, noise, filters, envelopes, LFOs, matrix and insert effects as for Tunefish 4
wherever the laws agree. Matrix point targets SHALL become shape targets, ADSR-decay targets envelope decay targets,
and MOD1–4 sources controllers CC2–CC5.

#### Scenario: Sub-oscillator
- **WHEN** an instrument with the sub-oscillator on and a matrix route to point 2's value is converted
- **THEN** the patch has two `points` slots with the same points, the second one octave lower at half gain, and both are targeted by the route

### Requirement: Tunefish 3 player mode
By default the converter SHALL drop the modules the Tunefish 3 player build leaves out (band-pass and notch filters,
MOD1–4, chorus, flanger, EQ), with one warning each. `--editor` SHALL keep them.

#### Scenario: Player mode drops chorus
- **WHEN** an instrument with a chorus in effect slot 2 is converted without `--editor`
- **THEN** its track has no chorus device and the converter warns naming the instrument and slot

#### Scenario: Editor mode keeps chorus
- **WHEN** the same instrument is converted with `--editor`
- **THEN** its track has a Tunefish-mode chorus device after the instrument

### Requirement: Tunefish 3 preset conversion
`synth-convert tf3 <program.txt> <out.chunk>` SHALL convert one Tunefish 3 preset file (`name;value` lines) to a
kind-64 chunk, with the same mapping as projects.

#### Scenario: Preset
- **WHEN** `program0.txt` of the Enigma Studio 3 presets is converted
- **THEN** the chunk parses, and a phrase rendered with it passes parity against the reference with the same preset

### Requirement: Tunefish 3 parity
The parity tool SHALL convert and compare synthetic one-instrument Tunefish 3 songs (one per module, mode and profile)
and Transplant against the reference renderer built from an external Enigma Studio 3 checkout, in player mode, with
the shared metrics: 1 dB loudness, 3 dB per octave band 63 Hz–16 kHz, onsets within 384 frames. It SHALL skip with a
message when the checkout is absent and commit its report (metrics only).

#### Scenario: Transplant
- **WHEN** Transplant is rendered by the reference and, converted, by `unisynth`
- **THEN** all three metrics are within tolerance

#### Scenario: Checkout absent
- **WHEN** `$TF3_REF` holds no Enigma Studio 3 checkout
- **THEN** the tool prints a skip message and exits 0, and the normal test suite does not depend on it

### Requirement: Tunefish 3 capture check
The parity tool SHALL compare a converted song against a local audio capture of the released intro: it aligns them
per 30 s window with the shared onset cross-correlation, normalises loudness, and reports per-window lag and
correlation, and octave bands up to the 8 kHz band. Timing passes when at least 90 % of the windows correlate ≥ 0.5 at
a lag within 2 blocks of the common offset (peak-picked onsets are unreliable on a lossy capture). It SHALL skip with
a message when the capture is absent.

#### Scenario: Transplant capture
- **WHEN** converted Transplant is compared with its capture
- **THEN** the report lists a lag for every window, every band is within 4 dB, and the timing check passes

#### Scenario: Capture absent
- **WHEN** `$TF3_CAPTURES` holds no capture for the song
- **THEN** the tool prints a skip message and exits 0

### Requirement: Tunefish 3 material stays local
No Enigma Studio 3 source, project, preset, extracted image, song blob converted from them, render or capture SHALL be
committed. Tests SHALL use synthetic projects written by a test helper.

#### Scenario: Repository check
- **WHEN** the change is complete
- **THEN** no tracked file contains Enigma Studio 3 code, projects, presets, extracted images, renders or captures

### Requirement: Released Tunefish 3 intro songs
An adapted `song.json` of a released intro committed under `project/music/released/` SHALL follow the existing
released-music rule (`project/music/NOTICE.md`): meta naming the production, its authors and the parity, published
pending the authors' permission, removed at their request.

#### Scenario: Released adapted songs
- **WHEN** `tools/tf3-extract/make-demos.py` writes Transplant and Train Control
- **THEN** each `project/music/released/<slug>.song.json` loads, carries meta (title, author, description with parity,
  copyright, url, adapted_from) and is listed in `project/music/NOTICE.md`

### Requirement: Tunefish 3 presets in the library
The library example SHALL convert every distinct Tunefish 3 preset in editor mode (identical parameter sets once, the
lowest program number keeps it) with its name, and the DAW server SHALL list the converted ones as a source. MOD1–4
inputs that a route reads SHALL start at 1.

#### Scenario: Preset parity
- **WHEN** each distinct preset plays 16 notes in the original editor and in the conversion
- **THEN** every preset is within 1.5 dB loudness, and the report records the per-band result

#### Scenario: MOD input before an effect word
- **WHEN** a preset routes MOD3 to volume and no note sets MOD3
- **THEN** the converted preset sounds at the original's level

