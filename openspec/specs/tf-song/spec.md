# tf-song Specification

## Purpose
Plays Tunefish 4 `.tfm` songs with the `tf-synth` engine, so that music authored in the Tunefish 4 plugin can be embedded in the intro and rendered to PCM. Includes verification against the original player and a compo size report.

## Requirements

### Requirement: .tfm decoding
The song reader SHALL decode `.tfm` bytes in place without allocation: u16 instrument count I (≤ 32), u16 tempo, I × u16 event counts, `INST`, I × 112 u8 parameters (`p = byte/100`), `SONG`, per instrument the arrays u16 row deltas, u8 notes, u8 velocities, then `ENDS`. All fields are little-endian.

#### Scenario: Valid song
- **WHEN** a well-formed `.tfm` is decoded
- **THEN** the instrument count, tempo, per-instrument patches and event lists match the file

#### Scenario: Malformed song
- **WHEN** a tag is wrong, I > 32, or an array runs past the end of the buffer
- **THEN** decoding returns an error and never panics or reads out of bounds

### Requirement: Event scheduling
Events SHALL be timed at 4 rows per beat (`row · 60 / (tempo · 4)` seconds, plus a caller start offset). Every event whose time falls inside a block's window SHALL fire at the start of that block, per instrument in stored order. Velocity 0 is note-off, 1..127 note-on, and note 0 is ignored.

#### Scenario: Block quantization
- **WHEN** an event lies 3 ms after a block start at 44.1 kHz
- **THEN** it takes effect at that block's first sample

#### Scenario: Note zero ignored
- **WHEN** an event has note 0
- **THEN** no voice changes

### Requirement: Song length
Song length SHALL be the latest last-event time over all instruments that have events. Instruments without events SHALL be skipped, and a song without any events SHALL have length 0.

#### Scenario: Empty instrument
- **WHEN** a song has an instrument with zero events
- **THEN** the length comes from the other instruments and nothing crashes

### Requirement: Song rendering API
The player SHALL render a decoded song block by block into a caller buffer, as interleaved stereo i16 with the reference player mix (instrument mix ×11, output ×(2000 · volume), default volume 0.1, clamped) or as f32. It SHALL support start, seek and stop (all-notes-off) and per-instrument mute. Muted instruments keep rendering their tails.

#### Scenario: Full prerender
- **WHEN** the caller renders `trunc(44100·T/512) + 1` blocks for a song of length T
- **THEN** the buffer holds the whole song in interleaved stereo

#### Scenario: Seek
- **WHEN** the caller seeks to time t
- **THEN** all voices are released and the next block starts at t

### Requirement: Reference parity verification
An opt-in tool SHALL render a `.tfm` with our player and with the original Tunefish 4 player, and compare them. Integrated loudness SHALL match within 1 dB, the long-term average spectrum within 3 dB per octave band (63 Hz–16 kHz), and note onsets within one block. GPL code, reference renders and third-party songs SHALL NOT be committed.

#### Scenario: Parity pass
- **WHEN** the tool runs on a song whose patches use only implemented features
- **THEN** it reports per-metric values and passes within tolerance

#### Scenario: Reference unavailable
- **WHEN** the reference renderer is not built or no songs are configured
- **THEN** the tool reports a skip, and the normal test suite does not depend on it

### Requirement: Compo size report
The build SHALL report the packed size of the engine and song player for the i686 compo target, with all effect features and with none, using the existing packer harness.

#### Scenario: Size report
- **WHEN** the size report runs
- **THEN** it prints raw and packed bytes for both feature sets
