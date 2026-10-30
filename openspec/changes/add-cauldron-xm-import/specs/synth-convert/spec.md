# Spec Delta

## ADDED Requirements

### Requirement: Mini-cauldron XM import
`synth-convert xm <in.xm> <patches.json> <out.song>` SHALL convert an XM made by mini-cauldron (TBL Jizz, Stash
song 1, Stash BLUISH), given that intro's decoded patch data, into a unified song that holds no generated sample data.

#### Scenario: Jizz converts
- **WHEN** the Jizz XM and `patches_jizz.json` are converted
- **THEN** the converter writes a song that loads and renders in `unisynth`, and that song holds no sample data for any `Probe` sample

### Requirement: Generated sample identification
The converter SHALL identify every generated sample as a (record, variant) of the patch data by the order rule the
reference renderers use: Jizz by sample name `ProbeNV`, Stash by XM sample order with the drum slot consuming the
drum count.

#### Scenario: Stash slot order
- **WHEN** Stash BLUISH is converted
- **THEN** its first four generated samples map to record 0, variants 0–3, and the next six samples are the stored drums

### Requirement: XM import errors
An XM effect or feature outside the set these songs use, a generated sample the patch data does not cover, or a patch
file of the wrong version SHALL be an error that names it, exits non-zero and writes nothing.

#### Scenario: Unsupported effect
- **WHEN** an XM pattern uses an effect the replay does not implement
- **THEN** the converter names the effect, pattern, row and channel, exits non-zero and writes no output

### Requirement: Phrases become notes
Every trigger of a generated sample SHALL become the notes of its 16-step phrase:
- **Untied step:** a note at the step's pitch, carrying its retrigger count and accent, and its phrase position.
- **Tied step:** a legato note that slides from the previous pitch over the first half of the step.
- **Note ends:** at the step end, or after the v2 gate's hold time.

#### Scenario: Tied steps slide
- **WHEN** a Jizz phrase whose step 2 is tied from step 1 is converted
- **THEN** step 2 is a legato note whose pitch moves linearly from step 1's pitch to its own over 1764 internal frames, and step 1's note is not released before step 2 begins

### Requirement: Phrase pitch words play unsigned
A step's note SHALL play its pitch word's unsigned frequency `w·R·ratio/65536`, so a word above 32767 steps the slot
as the original's. Over a v2 tie's glide window such a word SHALL be held signed.

#### Scenario: A word above 32767
- **WHEN** a Stash step holds pitch word 51199
- **THEN** its note plays 51199·R/65536 Hz (reached with the slot's coarse pitch when outside the note range), not the alias 14337

### Requirement: Phrase time follows the playback ratio
A trigger's playback ratio SHALL scale both the pitch and the step length, as the tracker's resampling does. Sample
offset, retrigger and loop jumps SHALL continue the phrase from the step under the new position.

#### Scenario: Octave-down trigger
- **WHEN** a phrase is triggered one octave below its native note
- **THEN** its notes are one octave lower and every step lasts twice as long

#### Scenario: Sample offset
- **WHEN** a phrase is triggered with a sample offset that lands in the middle of step 5
- **THEN** the first note is step 5's, lasting the rest of that step

### Requirement: Compounding cutoff per note
Each note's filter base and envelope amount SHALL follow the original per-retrigger compounding of the patch, with
retriggers counted as the original renderer counts them.

#### Scenario: Later variant is brighter
- **WHEN** variant 2 of a record with `mul` above 1 is converted
- **THEN** the base cutoff of its first note is higher than that of variant 1's first note, by the compounding the original applies over the retriggers between them

### Requirement: Mini-cauldron instruments
Each record SHALL map to a unified instrument whose voice is:
- a cauldron oscillator with the record's wave, voices and version
- the cauldron filter with the record's damping and decay, and per-note base and envelope amount
- the record's gain
- a full-scale output clip

#### Scenario: Rising cutoff
- **WHEN** a record whose decay is above 1 plays a tie chain
- **THEN** its filter opens over the chain, as the original's does

#### Scenario: Clipping patch
- **WHEN** a Stash record with gain above 10 is converted
- **THEN** the instrument's output stage clips at full scale

### Requirement: Decimated phrases keep their bandwidth
A phrase the original decimated SHALL be low-passed at its stored sample's Nyquist frequency, scaled by the playback
ratio. No decimation aliasing SHALL be reproduced.

#### Scenario: Quarter-rate phrase
- **WHEN** a Stash phrase stored at decimation 4 is converted
- **THEN** its track ends with a low-pass at R/8 times the ratio, about 4.4 kHz at native speed, after the clip and the other effects

### Requirement: Track effects follow the ratio
The record's echo, comb, doubler and wrap distortion SHALL map to effects on the track, with delay times scaled by the
track's playback ratio.

#### Scenario: Echo time follows the ratio
- **WHEN** a record with echo is triggered at native speed on one channel and one octave up on another
- **THEN** the two tracks' echo delays are 3·3529 internal frames at the native rate and half of that

### Requirement: Track grouping
Triggers of one record on one XM channel at one playback ratio (rounded to a semitone) SHALL share a track. Tracks
whose instrument and effects are equal and whose notes never overlap MAY be merged.

#### Scenario: Two channels, two ratios
- **WHEN** the same record plays on two channels at two different ratios
- **THEN** the song has two tracks for that record

### Requirement: Stored samples
The XM's stored samples (Stash's drum kit, Jizz's vocal and clap samples) SHALL become sampler instruments with their
loop settings. Their notes and gain SHALL follow the XM, like any other sample.

#### Scenario: Drum kit
- **WHEN** Stash song 1 is converted
- **THEN** its seven stored drum samples are the only sample data in the song

### Requirement: XM control replay
The converter SHALL replay the XM's control flow with FastTracker 2 semantics:
- order list, speed and BPM
- pattern break and jump
- note delay, note cut and retrigger
- sample offset
- volume column, volume slides and set volume
- panning, volume and panning envelopes with sustain and loop, and fadeout
- pitch slides and tone portamento
- relative note and finetune on the linear frequency table
- forward and ping-pong loops

#### Scenario: Note delay
- **WHEN** a row triggers a phrase with `ED3` at speed 6
- **THEN** the phrase's first note starts 3 ticks after the row start

### Requirement: XM levels become automation
Gain (sample volume, volume column, slides, envelopes, fadeout) and pan SHALL become automation of the track's
instrument. Pitch slides within a trigger SHALL become pitch-bend automation, while the step clock follows the sliding
ratio.

#### Scenario: Tone portamento on a phrase
- **WHEN** a phrase is sliding with `3xx` towards a note 5 semitones up
- **THEN** the notes' pitch rises along the slide as bend automation, and the steps shorten as the ratio grows

### Requirement: Ping-pong loops as reversed step order
When playback enters the backward pass of a ping-pong loop, the converter SHALL emit the loop region's steps in
reverse order, each playing forwards, and print one warning per affected sample.

#### Scenario: Pad past its end
- **WHEN** a Stash pad with a ping-pong loop is held past the end of its sample
- **THEN** the notes continue alternately through the loop region forwards and in reversed step order until the note ends, and a warning names the sample

### Requirement: Mini-cauldron song parity
The parity tool SHALL compare the three converted songs against `openmpt123` renders of the XMs rebuilt from the
bit-exact reference renders, using the shared metrics (1 dB loudness, 3 dB per octave band 63 Hz–16 kHz, onsets
within 100 frames). It SHALL report the seconds played through ping-pong loops per song.

#### Scenario: Jizz parity
- **WHEN** the Jizz reference and the converted Jizz song are rendered
- **THEN** all three metrics are within tolerance

### Requirement: Mini-cauldron phrase parity
The parity tool SHALL compare every phrase converted alone at native speed with its reference render using the shared
metrics, judging only the octave bands lying wholly below the phrase's stored Nyquist frequency. A decimated phrase's reference SHALL be
the original before its decimation, cut at the stored Nyquist.

#### Scenario: One phrase
- **WHEN** Jizz `Probe11` is converted alone and rendered
- **THEN** its loudness, octave bands up to 4 kHz (the last one wholly below its 8.86 kHz Nyquist) and onsets are within tolerance of the reference phrase

### Requirement: Mini-cauldron parity material stays local
When `research/tbl-mini-cauldron/out` is absent, the suite SHALL skip with a message. Its report SHALL be committed
(metrics only). Audio and converted songs SHALL NOT be committed.

#### Scenario: Reference material absent
- **WHEN** `research/tbl-mini-cauldron/out` does not exist
- **THEN** the suite prints a skip message and exits 0
