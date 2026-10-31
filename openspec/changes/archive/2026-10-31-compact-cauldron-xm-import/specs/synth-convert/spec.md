## MODIFIED Requirements

### Requirement: Mini-cauldron XM import
`synth-convert xm <in.xm> <patches.json> <out.song> [options]` (`--names`, `--controls`, `--thin`, `--keep-layers`, `--split-pitches`, `--dry`) SHALL convert an XM made by mini-cauldron (TBL Jizz, Stash song 1, Stash BLUISH), given that intro's decoded patch data, into a unified song that holds no generated sample data and whose every effect parameter lies in 0..1. `--names` SHALL write the track names (master excluded) as a JSON array; `--controls` SHALL set how often XM volume, pan and pitch reach the song (default `tick`). By default the converter SHALL drop layered triggers and merge pitch tracks (Requirement "Layered triggers dropped", "Track grouping"); `--keep-layers` and `--split-pitches` SHALL keep them, and with both the output SHALL be byte-identical to the conversion before this change. By default the dropped layers SHALL come back as effects on their tracks (Requirement "Dropped layers as track effects"); `--dry` SHALL leave them out.

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

### Requirement: Track grouping
By default, phrase triggers of one record SHALL share a track whatever their playback ratio. A trigger SHALL join the first track of its record (in time order) whose triggers it never overlaps and whose sounding pitch range, together with its own, spans at most one track's reach (`2·MAX_SHIFT − 2` semitones). The track class SHALL be its triggers' most frequent class, moved into the range every trigger reaches. Stored-sample triggers SHALL pack into tracks per (instrument, sample) by the same no-overlap rule, whatever their channel. With `--split-pitches`, triggers of one record on one XM channel at one playback ratio (rounded to a semitone) SHALL share a track, and tracks whose instrument and effects are equal and whose notes never overlap MAY be merged.

#### Scenario: Two channels, two ratios
- **WHEN** the same record plays one note on two channels at two ratios an octave apart, one after the other
- **THEN** the default song has one track for that record, and the song with `--split-pitches` has two

#### Scenario: Out of reach
- **WHEN** the same record plays two notes 100 semitones apart, one after the other
- **THEN** the song has two tracks for that record

### Requirement: Mini-cauldron song parity
The parity tool SHALL compare the three songs converted with `--keep-layers --split-pitches` against `openmpt123` renders of the XMs rebuilt from the
bit-exact reference renders, using the shared metrics (1 dB loudness, 3 dB per octave band 63 Hz–16 kHz, onsets
within 100 frames). It SHALL report the seconds played through ping-pong loops per song.

#### Scenario: Jizz parity
- **WHEN** the Jizz reference and the converted Jizz song are rendered
- **THEN** all three metrics are within tolerance

## ADDED Requirements

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
