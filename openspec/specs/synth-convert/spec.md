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
The converter SHALL print the cargo features (oscillator types, filter types and forms, envelope and LFO laws, quirk
flags, effect kinds and modes) that a unified song needs, so a compo build links only those modules. A build with
exactly the listed features SHALL load and render the song.

#### Scenario: Minimal features
- **WHEN** a song that uses only Slaughter-equivalent instruments and no effects is analysed
- **THEN** the list holds the saw/pulse, noise, state-variable filter and seconds-law envelope features only (the instrument itself is always built)

#### Scenario: Automation reach
- **WHEN** a lane steps a filter's type, raises the noise level or switches an envelope's mode
- **THEN** the list holds the modules of every value the lane can reach, and a build with exactly the list loads the song and plays all of them

#### Scenario: Quirks listed only when set
- **WHEN** a version 6 V2 song that sets no quirk flag is analysed
- **THEN** the list holds no quirk feature

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
`synth-convert xm <in.xm> <patches.json> <out.song> [options]` SHALL convert an XM made by mini-cauldron (TBL Jizz, Stash song 1, Stash BLUISH), given that intro's decoded patch data, into a unified song that holds no generated sample data and whose every effect parameter lies in 0..1. `--names` SHALL write the track names (master excluded) as a JSON array; `--controls` SHALL set how often XM volume, pan and pitch reach the song (default `tick`). Layout options: "Compact layout options".

#### Scenario: Jizz converts
- **WHEN** the Jizz XM and `patches_jizz.json` are converted
- **THEN** the converter writes a song that loads and renders in `unisynth`, and that song holds no sample data for any `Probe` sample

#### Scenario: Converted songs pass the song check
- **WHEN** a converted song is imported with `song import` and its track names
- **THEN** `song check` reports it valid

#### Scenario: Track names
- **WHEN** an XM is converted with `--names`
- **THEN** a phrase track is named `<sample> (record r)` or `<sample> (record r, ±class)`, a stored one `<sample> (sample)`, `.smp` dropped and repeats numbered

#### Scenario: Control rate
- **WHEN** an XM is converted with `--controls row` or `--controls note`
- **THEN** each trigger's volume, pan and pitch change only at row starts, or hold their first tick's values, and the song is smaller than with `tick`

#### Scenario: Thinned lanes
- **WHEN** an XM is converted with `--thin 4`
- **THEN** every thinned lane stays within 4 bytes of the per-tick steps at every frame, a jump of more than 4 bytes stays an exact step, and the song has fewer automation points than without

#### Scenario: Compact by default
- **WHEN** each of the three songs is converted without options
- **THEN** it has fewer tracks and a smaller xz size than with `--keep-layers --split-pitches`, and its loudness stays within 1.5 dB of the reference

#### Scenario: Old layout on request
- **WHEN** a song is converted with `--keep-layers --split-pitches`
- **THEN** the song is byte-identical to the conversion before this change

### Requirement: Generated sample identification
The converter SHALL identify every generated sample as a (record, variant) of the patch data by the order rule the
reference renderers use: Jizz by sample name `ProbeNV`, Stash by XM sample order with the drum slot consuming the
drum count.

#### Scenario: Stash slot order
- **WHEN** Stash BLUISH is converted
- **THEN** its first four generated samples map to record 0, variants 0–3, and the next six samples are the stored drums

### Requirement: XM import errors
An XM effect or feature outside the set the replay implements (requirement "XM control replay"), a generated sample
the patch data or generator map does not cover, or a patch file of the wrong version SHALL be an error that names it,
exits non-zero and writes nothing.

#### Scenario: Unsupported effect
- **WHEN** an XM pattern uses an effect the replay does not implement
- **THEN** the converter names the effect, pattern, row and channel, exits non-zero and writes no output

#### Scenario: Unsupported volume column
- **WHEN** an XM cell's volume column is a vibrato (`0xA0–0xBF`) or tone portamento (`0xF0–0xFF`) command
- **THEN** the converter names the volume column value, pattern, row and channel, exits non-zero and writes no output

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
- a clocked oscillator with the record's wave, voices and version
- the clocked filter with the record's damping and decay, and per-note base and envelope amount
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
By default, phrase triggers of one record SHALL share a track whatever their playback ratio. A trigger SHALL join the first track of its record (in time order) whose triggers it never overlaps and whose sounding pitch range, together with its own, spans at most one track's reach (`2·MAX_SHIFT − 2` semitones). The track class SHALL be its triggers' most frequent class, moved into the range every trigger reaches.

#### Scenario: Two channels, two ratios
- **WHEN** the same record plays one note on two channels at two ratios an octave apart, one after the other
- **THEN** the default song has one track for that record, and the song with `--split-pitches` has two

#### Scenario: Out of reach
- **WHEN** the same record plays two notes 100 semitones apart, one after the other
- **THEN** the song has two tracks for that record

### Requirement: Stored samples packed and split pitches
Stored-sample triggers SHALL pack into tracks per (instrument, sample) by the no-overlap rule of "Track grouping", whatever their channel. With `--split-pitches`, triggers of one record on one XM channel at one playback ratio (rounded to a semitone) SHALL share a track, and tracks whose instrument and effects are equal and whose notes never overlap MAY be merged.

#### Scenario: Stored sample on three channels
- **WHEN** one stored sample plays on three channels one after the other
- **THEN** the default song has one track for it

### Requirement: Compact layout options
By default the converter SHALL drop layered triggers, merge pitch tracks, put the dropped layers back as track effects and slim the effect chains. `--keep-layers` and `--split-pitches` SHALL keep the layers and the per-ratio tracks, and `--exact-chain` the whole chain; with all three, the output SHALL be byte-identical to the conversion before change compact-cauldron-xm-import. `--dry` SHALL leave the track effects out.

#### Scenario: Options parse
- **WHEN** `synth-convert xm` gets `--keep-layers`, `--split-pitches`, `--dry` or `--exact-chain`
- **THEN** it converts with that option, and an unknown option prints the usage

### Requirement: Stored samples
The XM's stored samples (Stash's drum kit, Jizz's vocal and clap samples) SHALL become sampler instruments with their
loop settings. Their notes and gain SHALL follow the XM, like any other sample.

#### Scenario: Drum kit
- **WHEN** Stash song 1 is converted
- **THEN** its seven stored drum samples are the only sample data in the song

### Requirement: XM control replay
The converter SHALL replay the XM's control flow with FastTracker 2 semantics:
- order list, speed and BPM (`F00` does nothing, as openmpt plays an XM)
- pattern break and jump
- note delay, note cut and retrigger
- multi retrig with volume change (`Rxy`, its x table as in FT2, the interval and volume memories kept per channel, the
  counter running across rows; a retrigger re-applies the row's set-volume column)
- sample offset
- volume column set volume, set panning and fine volume slide down/up (applied on tick 0); volume slides and set volume
- global volume and global volume slide (`Gxx`, `Hxy`)
- panning, panning slide (`Pxy`, on ticks after the first), volume and panning envelopes with sustain and loop, and
  fadeout
- pitch slides, extra-fine portamento (`X1x`/`X2x`, period ± x on tick 0, memories per channel) and tone portamento
  (which keeps the sounding note's instrument)
- instrument auto-vibrato (sine, square, ramp down, ramp up; depth ramped in over the sweep ticks from the note's start)
- relative note and finetune on the linear frequency table
- forward and ping-pong loops

#### Scenario: F00
- **WHEN** a row has `F00` at speed 6 and a note, and the next row another note
- **THEN** both notes start, 6 ticks apart, as in an `openmpt123` render

#### Scenario: Tone portamento with another instrument
- **WHEN** a channel's note of an instrument whose volume envelope has reached 0 meets `3xx` naming another instrument
- **THEN** no new trigger starts, and the sounding note replays its own instrument's envelope from the start

#### Scenario: Note delay
- **WHEN** a row triggers a phrase with `ED3` at speed 6
- **THEN** the phrase's first note starts 3 ticks after the row start

#### Scenario: Volume column fine slide
- **WHEN** a channel at volume 40 plays a row whose volume column is `0x83` (fine slide down 3) at speed 4, then a row
  with `0x92` (fine slide up 2)
- **THEN** its volume is 37 for every tick of the first row and 39 for every tick of the second

#### Scenario: Multi retrig
- **WHEN** a note whose volume column sets 48 plays `R43` at speed 6 (every 3 ticks; x = 4 is −8 in FT2's table), and
  the next row has `R43` without a volume column
- **THEN** new triggers of the same sample start at ticks 3 and 6, at volume 48 (the change, then the row's volume
  column set again) and 40, matching where and how loud an `openmpt123` render of the same XM retriggers

#### Scenario: Panning slide
- **WHEN** a channel at pan 128 plays `P04` for one row at speed 6
- **THEN** its pan is 108 at the row's last tick (5 ticks × 4 to the left)

#### Scenario: Auto-vibrato
- **WHEN** an instrument with sine auto-vibrato, depth 8, rate 16 and sweep 4 plays a held note
- **THEN** the note's frequency is unmodulated on tick 0 and its deviation grows over the first 4 ticks to the full
  depth, matching an `openmpt123` render of the same XM within 1 cent

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
The parity tool SHALL compare the three songs converted with `--keep-layers --split-pitches --exact-chain` against `openmpt123` renders of the XMs rebuilt from the
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

### Requirement: Per-note bytes as latched lanes
The mini-cauldron converter SHALL write the retrigger, phase and age bytes so that each note's byte decodes exactly from its step until `LATCH_WINDOW` frames later (or the next step), with as few points as a straight line between those windows allows; pitch bytes SHALL stay exact steps.

#### Scenario: Rising retrigger count
- **WHEN** a record's retrigger byte rises by 2 on each of 50 evenly spaced notes
- **THEN** its lane has at most 4 points and every note reads its own byte

### Requirement: Layered triggers dropped
By default the converter SHALL drop a trigger that repeats a kept one (same instrument, sample, XM note and sample offset, on another channel) when it starts at the same tick, or later while the kept one still sounds (before its sample ends, at most 2.5 s after it) at a lower first-tick volume. Triggers are taken in time order. The converter SHALL print `layers N dropped (P same-tick, E echoes)` on stderr.

#### Scenario: Stereo pair
- **WHEN** one note plays at full volume hard left on one channel and hard right on another at the same tick
- **THEN** one trigger remains

#### Scenario: Echo tap
- **WHEN** a note at volume 64 is followed 3 rows later by the same note at volume 32 on another channel while the first still sounds
- **THEN** the later trigger is dropped and the first is unchanged

#### Scenario: A louder or later note stays
- **WHEN** the same note follows on another channel at the same or a higher volume, or after the first has ended
- **THEN** both triggers remain

### Requirement: Absorbed layers keep their level
A kept trigger SHALL take the level of the same-tick copies it absorbs. Per side, the first-tick levels of copies at its own first-tick frequency SHALL add as amplitudes and those of detuned copies as powers. The sum SHALL set its pan (equal-power) and scale its volume on all its ticks.

#### Scenario: Centred pair
- **WHEN** a hard-left and a hard-right copy at full volume are absorbed into one
- **THEN** it plays centred at √2 times the volume

#### Scenario: Detuned pair
- **WHEN** a hard-left copy and a detuned copy panned 15/16 right, both at full volume, are absorbed into one
- **THEN** it plays at √2 times the volume, panned at the right side's share of the power

### Requirement: Dropped layers as track effects
Without `--dry`, a track SHALL get effects after its post gain for what its triggers' dropped layers did, and no extra track, when at least half of its triggers had layers. A phrase track SHALL get a small stereo reverb. The track volume SHALL keep the dry signal at its level.

#### Scenario: Echo taps
- **WHEN** most of a track's triggers had echo taps
- **THEN** it gets a two-tap stereo delay without feedback at the most common pattern's first left and first right tap times, each tap at the taps' RMS level relative to the dry signal, and the song tempo reaches the longest tap

#### Scenario: Jizz pads
- **WHEN** Jizz is converted
- **THEN** its pad tracks have a tempo delay with taps 3 and 6 rows after the note on opposite sides, and a reverb; the song's loudness stays within 1 dB of the reference

#### Scenario: Dry on request
- **WHEN** Jizz is converted with `--dry`
- **THEN** no track has a reverb, and the song has the same tracks

### Requirement: Slim effect chain
Without `--exact-chain`, a phrase track's band-limiting low-pass SHALL have one stage, and none when the track does not slide and its cut is at least 16 kHz. A clip whose input, in a solo render of the track's devices before it, never passes its threshold by more than 1e-4 SHALL be left out. The converter SHALL print `clips N idle left out` on stderr.

#### Scenario: One stage
- **WHEN** a record decimated by 2 plays natively
- **THEN** its track has one low-pass stage, one frame of latency less than with `--exact-chain`

#### Scenario: Inaudible cut
- **WHEN** that record plays an octave up without a slide
- **THEN** its track has no low-pass

#### Scenario: Idle clip
- **WHEN** a clip's input never reaches its threshold
- **THEN** the song has no device for it, and the track sounds as with it

### Requirement: Old-layout migration
A host-side migration SHALL convert unified instrument chunks and song files in the old parameter layout (chunk format
versions 1 and 2, or old parameter names) to the new layout. It SHALL set the shared-algorithm settings that replace
each old mode or kind, and run on import, so old songs and patches load in the DAW and the converter without
re-authoring.

#### Scenario: Old patch sounds the same
- **WHEN** a format version 1 chunk with a 4-pole V2-type filter, a V2-mode envelope and the V2 voice is migrated and played
- **THEN** its render passes the parity tolerances against the same chunk played by the build before this change

#### Scenario: Old song file
- **WHEN** a song file from `project/music/` written before this change is imported
- **THEN** it loads, re-exports in the new layout, and its render passes the parity tolerances against its render before this change

#### Scenario: Migrated once
- **WHEN** a `song/1` or `us-patch/1` file is loaded, saved as `/2`, and loaded again
- **THEN** the second load applies no migration and yields the same values

#### Scenario: New file with an old name
- **WHEN** a `song/2` file names a parameter only the old layout has (for example `env1.mode`)
- **THEN** loading fails with an error naming the parameter

#### Scenario: Lanes across a split
- **WHEN** an old song automates a parameter that the new layout splits (a filter type, an envelope slope under an automated law)
- **THEN** the migration writes one lane per changed setting, at the old lane's times, and the render passes the drift report like the rest of the song
- **AND** a lane or route on a naive generator's mode or ring fails with an error naming it

#### Scenario: No song left behind
- **WHEN** the format switch is complete
- **THEN** every tracked song in the inventory (design, "Song inventory") is in the new layout and loads in the new player
- **AND** the load-all check fails if any tracked song fails to load, is skipped, or if a tracked location holds fewer songs than recorded
- **AND** external and git-ignored corpora present on the machine are re-converted and checked, and absent ones are reported, not failed

### Requirement: Fidelity across the merge
After each merge of a source module into a shared algorithm, every song the converters produce SHALL still pass its
parity report within tolerance (1 dB loudness, 3 dB per octave band, onsets): the WaveSabre and Tunefish device items
and six reference songs, and the 15 V2 acceptance songs plus the aux songs. Each song's drift against the previous
build SHALL be recorded.

#### Scenario: V2 acceptance song after a merge
- **WHEN** kkrieger is converted and rendered after the envelope merge
- **THEN** its V2 parity report passes, and its loudness and band drift against the previous build are listed in the stage results

### Requirement: Source names stop at the converter
The converters SHALL map source modules (WaveSabre, Tunefish, V2 names, modes and eras) to the synth's functional
parameters, settings, laws and quirk flags. The synth's names SHALL not depend on the source.

#### Scenario: V2 envelope converted
- **WHEN** a V2 patch is converted
- **THEN** its envelopes use the `frame` law and exponential curves, and no parameter or option in the result is named after V2

### Requirement: Quirk survey
A tool SHALL list, for every song in the V2 corpus and the acceptance set, the quirk flags its converted patches set.
It SHALL test each quirk by rendering the songs that set it with the flag cleared, and write the table that documents
each quirk: source, format versions, songs, and whether a song leaves tolerance without it.

#### Scenario: Quirk table
- **WHEN** the survey runs over the corpus
- **THEN** the table lists each quirk flag with its source, the format versions that set it, the songs that use it and the songs that leave tolerance without it, and every flag the converter can set appears in it

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

### Requirement: Elevated song import
`synth-convert elevated <music.asm> <out.song> [--table 7|8] [tail-seconds]` SHALL read a generated `music.asm` of
Mentor's 4k synthesizer (Elevated, Atrium) and the `music.h` beside it:
- `music.asm`: the `DB`/`DD` data of `_tempo_data` (samples per step), `_pattern_data` (16 note bytes per pattern),
  `_sequence_data` (one pattern index per row per synth machine) and `_machine_tree`
- `music.h`: `NUM_ROWS`, the sequence stride the synth uses (`_tempo_data`'s row byte can differ)

The machine table differs per build: 8 entries (synth, delay, filter, compressor, mixer, sine, atan, allpass; Atrium)
or 7 without atan (Elevated, where opcode 6 is allpass). Without `--table`, the converter SHALL walk the tree with
both tables and take the one that ends exactly on the terminator with one stack entry. A tree that reads the same
under both (no opcode above 5) is not ambiguous. The converter SHALL exit with an error asking for `--table` when both
walk to different machines, or when neither walks. It SHALL
write a `synth-mix` song blob with only unified kinds. A missing section, a truncated machine, an unknown machine
opcode, a pattern index out of range, a zero-length envelope segment, or a stack that underflows or does not end with
exactly one entry SHALL exit non-zero with a message naming the cause, and write nothing.

#### Scenario: Atrium converts
- **WHEN** Atrium's `music.asm` is converted
- **THEN** the output loads and renders in `unisynth` for `rows·16·step` frames plus the tail

#### Scenario: Table detection
- **WHEN** Elevated's and Atrium's files are converted without `--table`
- **THEN** Elevated is read with the 7-entry table and Atrium with the 8-entry table

#### Scenario: Unknown opcode
- **WHEN** the machine tree holds opcode 9
- **THEN** the converter names the opcode and its byte offset, exits non-zero and writes no output

### Requirement: Machine tree routing
The converter SHALL run the machine tree as the original does, as a stack of tracks:
- The first machine is always a synth.
- A synth pushes a new track holding its instrument.
- An effect machine appends its device to the chain of the top track.
- A mixer with gains `(gTop, gBelow)` pops two tracks and pushes a new track that receives the top track scaled by
  `gTop` and the one below scaled by `gBelow`.
- After the last machine (a negative opcode byte), the single remaining track SHALL be the master.

Gains SHALL be carried by track volume, or by a gain device when a volume cannot carry them (for example a negative
gain).

#### Scenario: Mixer
- **WHEN** the tree is synth A, delay, synth B, mixer (0.5, 1)
- **THEN** the song has track A (instrument, delay), track B (instrument) and a master receiving B at 0.5 and A at 1

### Requirement: Elevated note rules
Synth machine k SHALL read its rows from `_sequence_data[k·rows …]`. Row r, step s SHALL be at frame
`(16·r + s)·step`. A note byte n in 1–126 SHALL start MIDI note n. Each channel SHALL be monophonic: a new note cuts the
sounding one at once. Note `0x7F` SHALL cut the sounding note without starting one: the converter emits a note-off,
which ends a linear envelope at once. Note 0 SHALL leave the sounding note playing. A note's envelope SHALL be the
linear timed envelope (attack, decay, hold = the machine's sustain frames, release; sustain level ½), so it ends on
its own after `attack + decay + sustain + release` frames with no note-off.

#### Scenario: Long note over empty steps
- **WHEN** a channel holds a note whose envelope lasts 3 steps, followed by two empty steps
- **THEN** the note plays its whole envelope

#### Scenario: Cut
- **WHEN** the step after a note holds `0x7F`
- **THEN** the note is silent from that step's frame

### Requirement: Elevated instrument mapping
Each synth machine SHALL become one unified instrument:
- The 3 oscillator pairs become 3 `pair` slots: waveform, combine (2 add, 4 multiply), phase shift, detune.
- Envelope 1 is linear: attack, decay, hold and release from the machine's segment lengths, sustain ½. Capacity 1,
  steal oldest.
- A pair's op 3 (subtract) is an added pair with its polarity set; op 1 (off) turns the slot off.
- `base_freq` and `freq_exp` become the pitch drop: base in Hz from cycles per sample, `k = freq_exp`.
- The noise mix becomes the noise level; noise is added before the envelope.
- Volume and stereo give `L = vol·x` and `R = vol·stereo·x`.

A setting the instrument cannot express exactly (for example a negative stereo factor) SHALL produce a warning naming
the machine, and the converter SHALL take the closest mapping.

#### Scenario: Pair mapping
- **WHEN** a synth machine's second pair is a square with op 3 and detune 1.01
- **THEN** slot 2 is a pair square, added, with its polarity set and detune 1.01

### Requirement: Elevated effect mapping
The filter machine SHALL become EQ `svf-lfo`. The sine and atan distortion machines SHALL become `sine` and `atan`.
The compressor SHALL become `knee`. Delay SHALL become `ping-pong` and allpass SHALL become `allpass`, with length,
feedback and the machine's parameters carried unchanged.

#### Scenario: Effect chain order
- **WHEN** a synth is followed by filter, atan distortion and delay machines
- **THEN** its track chain is instrument, EQ `svf-lfo`, distortion `atan`, delay `ping-pong`

### Requirement: Elevated reference parity
`tools/elevated-parity` SHALL build the reference renderer outside the repo. It assembles the original `synth.asm` with
a song's `music.asm` (`nasm -f win32`) and links it to our MIT harness `render-ref.c`, which calls `generateMusic` and
writes `MusicBuffer` as a 16-bit stereo WAV. The renderer runs under Wine. `--solo <k>` SHALL zero the volume of every
other synth machine. `parity.ts` SHALL compare our render with the reference using the shared metrics and the
song-parity tolerances of `tools/unisynth-parity`: 1 dB loudness, 3 dB per octave band above a 24 dB floor, and
timing by onset-strength alignment (correlation ≥ 0.5). Peak-picked onsets are listed but not judged. When the original sources are absent, it SHALL skip with exit 0.
Atrium and Elevated SHALL pass, and the metric table SHALL be recorded in `research/elevated-synth.md`.

#### Scenario: Acceptance
- **WHEN** `parity.ts` runs with the Atrium and Elevated sources present
- **THEN** both songs pass those tolerances

#### Scenario: Solo channel
- **WHEN** the reference renders Atrium with `--solo 3`
- **THEN** the WAV holds only channel 3 after its effects, and our render of the song converted with `--solo 3` (every other synth machine at volume
  0, as the harness does) passes the same tolerances

#### Scenario: Sources absent
- **WHEN** `ELEVATED_SRC` and the default location hold no `synth.asm`
- **THEN** `parity.ts` prints that it skipped and exits 0

### Requirement: Elevated capture check
`tools/elevated-parity/captures.sh` SHALL fetch the YouTube captures of the released intros (Elevated `jB0vBmiTr6o`,
Atrium `jlRoOqRs0bs`) into `$ELEVATED_CAPTURES` (default `~/Downloads/64k-tools/elevated-captures`). The parity tool
SHALL compare each converted song, and the reference render, with its capture, using the Tunefish 3 capture
comparison (`tools/tf3-parity/capture.ts`):
- 30 s windows aligned by onset cross-correlation, with loudness normalised
- octave bands up to 8 kHz within 4 dB

Timing SHALL use a consistency rule (user decision 2026-11-10; the pad openings and breaks of these songs are
onset-poor): every window that correlates ≥ 0.5 lies within 2 blocks (768 frames) of those windows' median lag, and
at least 60 % of the windows correlate ≥ 0.5.

It SHALL skip with a message when a capture is absent.

#### Scenario: Reference matches the release
- **WHEN** the reference render of Atrium is compared with its capture
- **THEN** the timing check passes, which confirms that the local `music.asm` is the released song

#### Scenario: Converted song against the capture
- **WHEN** converted Elevated is compared with its capture
- **THEN** every band is within 4 dB and the timing check passes

#### Scenario: Capture absent
- **WHEN** `$ELEVATED_CAPTURES` holds no capture for the song
- **THEN** the tool prints a skip message and exits 0

### Requirement: Elevated material stays local
The original assembly, `music.asm`/`music.h` files, executables and WAVs SHALL NOT be committed. The reference build
SHALL write only below `/tmp` (or `$ELEVATED_REF`). Tests SHALL use synthetic `music.asm` text written in the test.

#### Scenario: Repo check
- **WHEN** the change is complete
- **THEN** `git ls-files` lists no `synth.asm`, `music.asm`, `.exe` or `.wav` from the Elevated sources

### Requirement: Generator map
`synth-convert xm <in.xm> <map.json> <out.song>` SHALL accept a generator map (`"format": "xm-gen/1"`) in place of
patch data, telling the formats apart by their keys. Per XM sample it states `stored`, or `generated` with an engine
id, its parameter record and its length (equal to the XM sample's). Patch files SHALL convert as before.

#### Scenario: Cauldron songs unchanged
- **WHEN** Jizz, Stash song 1 and Stash BLUISH are converted with their patch files before and after this change
- **THEN** the songs, and their renders, are byte-identical

#### Scenario: Map covers the XM
- **WHEN** a map lists fewer samples than the XM has non-empty ones, names a sample the XM does not have or has
  empty, or gives a generated length other than the XM's
- **THEN** the converter names the instrument and sample, exits non-zero and writes nothing

### Requirement: Generator map entries are complete
A generated entry SHALL carry its parameter record as an object, and a listed sample SHALL be non-empty in the XM.
Identical generated records SHALL share one instrument.

#### Scenario: Shared record
- **WHEN** two XM instruments' entries name the same engine, length and parameters
- **THEN** both play on one instrument

#### Scenario: Missing record
- **WHEN** a generated entry has no `params` object
- **THEN** loading the map fails naming the entry

### Requirement: Only stored samples are stored
A sample the map marks `stored` SHALL become a sampler instrument on its GSM-encoded PCM, as the mini-cauldron's stored
samples do. A sample marked `generated` SHALL never be written as sample data. If its engine is not available, the
conversion SHALL fail naming the engine and the sample.

#### Scenario: Stored and generated in one song
- **WHEN** an XM whose map marks instrument 7 stored and the rest generated is converted
- **THEN** the song's only sample data is instrument 7's

#### Scenario: Engine missing
- **WHEN** a map names an engine the converter does not implement
- **THEN** the converter names the engine and the first sample using it, exits non-zero and writes nothing

### Requirement: Generated triggers become notes
Every trigger of a generated sample SHALL become notes of an instrument running that sample's engine. Its phrase clock
SHALL follow the trigger's playback rate per tick (the track's class and bend). The note-on SHALL latch the start
position (the `9xx` offset, or 0); a loop re-entry SHALL restart the position at the loop start; the end of a
non-looped sample SHALL end the note. Volume, pan and pitch SHALL follow the XM replay as automation.

#### Scenario: Offset and loop
- **WHEN** a generated sample with a forward loop is triggered with `9xx` and held past its end
- **THEN** its note starts at frame `xx·256` of the generated sample and re-enters at the loop start each time the
  playback passes the loop end

#### Scenario: Engine-neutral path
- **WHEN** the trigger→note path is exercised with a test engine that only counts frames
- **THEN** the frame positions it reports equal the replay's resampled sample positions within one frame at every tick

### Requirement: Generated-note byte contract
For engines with one note per run of the sample, a generated note-on SHALL carry its start frame as `o::TRACKING`
byte × 4096 + `o::PM_OUT` byte × 16 (rounded) and a seekable engine's backward pass as `o::FEEDBACK` 255 (else 0),
written as latched lanes. Phrase engines define their own lanes; the mini-cauldron's bytes SHALL stay unchanged.

#### Scenario: Position bytes
- **WHEN** a generated sample is triggered with `9xx` = 0x13 (frame 4864)
- **THEN** at its note-on `o::TRACKING` reads byte 1 and `o::PM_OUT` byte 48 (4096 + 48 × 16 = 4864), `o::FEEDBACK` 0

### Requirement: Generated notes leave room for their bytes
A generated note whose contract bytes differ from the previous note's SHALL start more than `LANE_LEAD` frames after
it. When one starts closer, the converter SHALL drop the earlier note-on. If that note-on re-entered a running note (an
off and an on at the same frame), the running note plays on instead. The later note-on is kept, so a trigger's onset
stays exact. Notes with the same bytes may follow as closely as they come.

#### Scenario: A cut on a short ping-pong loop
- **WHEN** a 256-frame ping-pong drum re-enters its loop 59 frames before another trigger cuts it on the same track
  (Alien Sex Clone, song frame 923 808)
- **THEN** that re-entry is dropped, the drum plays on to the cut, and the new trigger's note-on reads its own bytes

#### Scenario: Too close
- **WHEN** two generated notes with different start positions are `LANE_LEAD` frames apart on one track
- **THEN** the first note-on is dropped and the conversion succeeds

### Requirement: Engine events unroll along the trigger
An engine that generates internal events (a phrase engine's notes) SHALL have them unrolled along the trigger's path
through an engine callback, as `phrase_events` does for the cauldron, so an event lands where the playback reaches its
frame.

#### Scenario: Phrase slowed down
- **WHEN** a phrase engine's sample with events at frames 0 and 1750 is played at half its native ratio
- **THEN** the second event's note starts 3500 frames of the original rate after the first, as the replay's positions
  say

### Requirement: Generator map extraction
`tools/xm-gen/` SHALL write the generator map of each supported intro from its unpacked executable image. The committed
maps SHALL hold only parameter numbers and the stored/generated assignment: no code, tables of the original program
beyond the song's own patch data, sample data or images.

#### Scenario: Maps reproduce the sample list
- **WHEN** a map is extracted from Theta's image
- **THEN** it marks instruments 1, 2, 3 and 18 stored and every other non-empty sample generated, with lengths equal to
  the XM's sample lengths of the compo build

### Requirement: Plain XM song parity
The parity tool SHALL compare each converted song against an `openmpt123` render of its XM in which every generated
sample is replaced by the reference render of the build the map was extracted from, with the shared metrics. A song
whose engine is not in the build SHALL be reported pending, not failed. (Per-sample parity of each engine arrives with
the engine's change.)

#### Scenario: Engine pending
- **WHEN** a map names an engine this build lacks
- **THEN** its row is pending and the run still exits 0

#### Scenario: Theta judged against its own build
- **WHEN** Theta's song is compared
- **THEN** the reference XM carries the 1998 compo executable's samples for instruments 4, 6, 7 and 8, not the
  later-build XM's

### Requirement: Plain XM parity material stays local
The parity suites SHALL read their material from `$ZENEKK_DIR` (default `~/Downloads/zenekk`) and SHALL skip with a
message when it is absent. Reports SHALL be committed with metrics only; XMs, images, renders and converted songs SHALL
NOT be committed.

#### Scenario: Material absent
- **WHEN** `$ZENEKK_DIR` is empty
- **THEN** the suites print a skip message and exit 0

### Requirement: Fudge engines
The generator-map engines `fm-shaper` and `sweep-drum` SHALL convert to a slot of type Shaper tone and Sweep drum.
- The map's raw arguments SHALL be written on the types' laws. Float arguments are bit patterns; the FM and curve
  flags are integers, read as integers. Each value SHALL round-trip exactly, or the conversion fails naming the
  instrument, the sample and the argument. The one exception is the drum's slope, which SHALL round-trip within
  2⁻²⁸ per frame.
- The drum's envelope end and call length SHALL be written as its slope `(start − end)/frames`.
- The carried shaper value (`carry`) and the tone's fourth argument SHALL be ignored.
- The playback clock `C` SHALL be written as the slot pitch `C/256` Hz at note 60.
- The note byte contract applies unchanged.

#### Scenario: Integer flag
- **WHEN** instrument 4's entry (FM flag raw `1`) converts
- **THEN** its slot's FM value is on

#### Scenario: Out-of-range argument
- **WHEN** a `sweep-drum` entry has a positive acceleration
- **THEN** the converter names the instrument, the sample and the argument, exits non-zero and writes nothing

#### Scenario: Converted song
- **WHEN** `synth-convert xm "Fudge - Alien Sex Clone.xm" tools/xm-gen/maps/fudge-asc.json out.song` runs
- **THEN** it succeeds, the song holds no PCM for the 22 generated samples, and their instruments load in a
  `us-osc` build

### Requirement: Engine tracks in the v3 layout
A source track whose oscillator type exists only in the v3 layout SHALL be built in the old layout with the clocked
type as a stand-in. After the song's own migration to v3, the converter SHALL set that slot's type. Songs without such
tracks SHALL be written as before.

#### Scenario: TBL unchanged
- **WHEN** the TBL identity guard runs
- **THEN** the songs and renders are byte-identical

#### Scenario: Engine lanes
- **WHEN** a generated note writes its start and backward bytes
- **THEN** the song's lanes for that track address the v3 `o::TRACKING`, `o::PM_OUT` and `o::FEEDBACK` of slot 1

### Requirement: Fudge per-sample parity
With `$ZENEKK_DIR` present, each of the 22 generated samples rendered by its engine at native clock SHALL meet both
targets against the same sample of `fudge-asc.reference.xm`:
- within 1 LSB on at least 99.9 % of frames;
- no frame off by more than 64 LSB.

The plain-XM parity SHALL judge the song with the shared metrics. Without `$ZENEKK_DIR` both skip.

#### Scenario: Sample parity
- **WHEN** the parity test runs with the reference present
- **THEN** it reports 22 of 22 samples passing

#### Scenario: Song row
- **WHEN** `tools/unisynth-parity/plain-xm.ts` runs with the reference present
- **THEN** the Fudge row is judged, not pending
