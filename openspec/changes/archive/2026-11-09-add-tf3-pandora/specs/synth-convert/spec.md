## ADDED Requirements

### Requirement: Older Tunefish 3 format extraction
`tools/tf3-extract/extract.py` SHALL find the older Tunefish 3 synth block (the Pandora-era layout, instruments stored
instrument-major) in an unpacked intro image and write it as a current-format `.e3prj`. A candidate SHALL be accepted
only when the instrument block and the whole song parse consistently. Reads SHALL stay inside the image, and nothing is
written for a partial parse.

#### Scenario: Pandora image
- **WHEN** the extractor runs on the unpacked Pandora image
- **THEN** it writes one `.e3prj` holding Pandora's instruments and its song (bpm 125), and `synth-convert e3prj`
  converts it without errors

#### Scenario: Truncated image
- **WHEN** the image is cut inside the older block
- **THEN** the extractor reports that no block was found and writes no file

#### Scenario: Current-format intros unchanged
- **WHEN** the extractor runs on the Transplant and Train Control images
- **THEN** it writes the same `.e3prj` files as before this change

### Requirement: Older Tunefish 3 parameter mapping
Each older-format instrument value SHALL map to the current Tunefish 3 parameter with the same meaning, converted to
the current value range. A current parameter the older engine lacks SHALL take a value at which it has no effect. The
mapping SHALL be documented per parameter in the functional spec. If an older-engine law cannot be expressed as a value,
the converter SHALL reproduce it behind an explicit older-engine option, and SHALL NOT change conversions of
current-format projects.

#### Scenario: Older-engine option
- **WHEN** an older-format project is converted with the older-engine option
- **THEN** unison, sub-oscillator passes, oscillator drive and the LFO sine follow the older engine's laws, and the
  conversion lists each older-engine behaviour the unified synth cannot play as a warning

#### Scenario: Missing effect slots
- **WHEN** an older-format instrument is mapped
- **THEN** every effect slot of the written instrument is off

#### Scenario: Current projects unaffected
- **WHEN** Transplant, Train Control and the preset bank are converted after this change
- **THEN** their song and patch output is byte-identical to before

### Requirement: Older-engine known differences
Older-engine behaviour the unified synth cannot play SHALL be a documented known difference: listed in the functional
spec and `docs/synth/tunefish3.md` with its measured effect on the Pandora capture check, and named in the conversion's
warnings. The known differences are the oscillator offset with its one-sided clip, clipping per unison layer and
sub-oscillator pass, the point curve played without its table, and the missing decimation stage.

#### Scenario: Known differences reported
- **WHEN** Pandora is converted with the older-engine option
- **THEN** the warnings name each known difference its instruments use, and the docs give each one's measured effect

### Requirement: Pandora capture acceptance
Converted Pandora SHALL pass the Tunefish 3 capture check against the local Pandora capture: every octave band up to
8 kHz within 4 dB after loudness normalisation, and timing agreement in at least 90 % of the 30 s windows. No reference
renderer exists for the older engine, so the capture is the only acceptance reference.

#### Scenario: Pandora against its capture
- **WHEN** the parity tool compares converted Pandora with `$TF3_CAPTURES/pandora.wav`
- **THEN** the report lists a lag for every window, every band is within 4 dB, and the timing check passes

#### Scenario: Capture absent
- **WHEN** `$TF3_CAPTURES` holds no Pandora capture
- **THEN** the Pandora item is skipped with a message and the run exits 0

### Requirement: Released Pandora song
An adapted `project/music/released/pandora.song.json` SHALL be committed only after the capture acceptance passes. It
SHALL follow the released-music rule: meta naming Pandora by Brain Control, music by Skyrunner, and the capture
parity, listed in `project/music/NOTICE.md`, published pending permission and removed at the authors' request.

#### Scenario: Pandora released
- **WHEN** `tools/tf3-extract/make-demos.py` writes Pandora
- **THEN** `project/music/released/pandora.song.json` loads, carries meta (title, author, description with the capture
  parity, copyright, url, adapted_from), and is listed in `project/music/NOTICE.md`
