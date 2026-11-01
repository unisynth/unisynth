# Spec Delta

## ADDED Requirements

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

## MODIFIED Requirements

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
