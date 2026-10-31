## MODIFIED Requirements

### Requirement: Compact layout options
By default the converter SHALL drop layered triggers, merge pitch tracks, put the dropped layers back as track effects and slim the effect chains. `--keep-layers` and `--split-pitches` SHALL keep the layers and the per-ratio tracks, and `--exact-chain` the whole chain; with all three, the output SHALL be byte-identical to the conversion before change compact-cauldron-xm-import. `--dry` SHALL leave the track effects out.

#### Scenario: Options parse
- **WHEN** `synth-convert xm` gets `--keep-layers`, `--split-pitches`, `--dry` or `--exact-chain`
- **THEN** it converts with that option, and an unknown option prints the usage

### Requirement: Mini-cauldron song parity
The parity tool SHALL compare the three songs converted with `--keep-layers --split-pitches --exact-chain` against `openmpt123` renders of the XMs rebuilt from the
bit-exact reference renders, using the shared metrics (1 dB loudness, 3 dB per octave band 63 Hz–16 kHz, onsets
within 100 frames). It SHALL report the seconds played through ping-pong loops per song.

#### Scenario: Jizz parity
- **WHEN** the Jizz reference and the converted Jizz song are rendered
- **THEN** all three metrics are within tolerance

## ADDED Requirements

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
