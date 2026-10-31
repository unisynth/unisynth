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

### Requirement: Mini-cauldron XM import
`synth-convert xm <in.xm> <patches.json> <out.song> [--names <tracks.json>]` SHALL convert an XM made by mini-cauldron (TBL Jizz, Stash song 1, Stash BLUISH), given that intro's decoded patch data, into a unified song that holds no generated sample data and whose every effect parameter lies in 0..1. With `--names` it SHALL write one name per track (master excluded) as a JSON array.

#### Scenario: Jizz converts
- **WHEN** the Jizz XM and `patches_jizz.json` are converted
- **THEN** the converter writes a song that loads and renders in `unisynth`, and that song holds no sample data for any `Probe` sample

#### Scenario: Converted songs pass the song check
- **WHEN** a converted song is imported with `song import` and its track names
- **THEN** `song check` reports it valid

#### Scenario: Track names
- **WHEN** an XM is converted with `--names`
- **THEN** a phrase track is named `<sample> (record r)` or `<sample> (record r, ±class)`, a stored one `<sample> (sample)`, `.smp` dropped and repeats numbered

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
track's playback ratio. A song0 special SHALL become a wrap only where the original's stored sample wraps: slot 9
(×1.25). The +48 of slots 11 and 15 never wraps there (the spike wave's DC keeps every value under 80), so it is a DC
shift and SHALL be dropped.

#### Scenario: A special that never wraps
- **WHEN** Stash song0's record 10 (slot 11, +48) is converted
- **THEN** its track has no wrap effect, while record 8 (slot 9, ×1.25) ends its chain with a wrap of drive 1.25

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
- pitch slides and tone portamento (which keeps the sounding note's instrument)
- relative note and finetune on the linear frequency table
- forward and ping-pong loops

#### Scenario: Tone portamento with another instrument
- **WHEN** a channel's note of an instrument whose volume envelope has reached 0 meets `3xx` naming another instrument
- **THEN** no new trigger starts, and the sounding note replays its own instrument's envelope from the start

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
