# Spec Delta

## ADDED Requirements

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

## MODIFIED Requirements

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
