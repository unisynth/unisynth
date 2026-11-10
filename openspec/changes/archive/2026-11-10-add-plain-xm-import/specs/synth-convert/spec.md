# Spec Delta

## ADDED Requirements

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
it; otherwise the conversion SHALL fail naming the frame.

#### Scenario: Too close
- **WHEN** two generated notes with different start positions are `LANE_LEAD` frames apart on one track
- **THEN** the conversion fails naming the song frame, and nothing is written

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

## MODIFIED Requirements

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
