## MODIFIED Requirements

### Requirement: Elevated note rules
Synth machine k SHALL read its rows from `_sequence_data[k·rows …]`. Row r, step s SHALL be at frame
`(16·r + s)·step`. A note byte n in 1–126 SHALL start MIDI note n. Each channel SHALL be monophonic: a new note cuts the
sounding one at once. Notes `0x7F` and `0xFF` SHALL cut the sounding note without starting one: the converter emits a
note-off, which ends a linear envelope at once. Notes 0 and `0x80` SHALL leave the sounding note playing. A note's
envelope SHALL be the linear timed envelope (attack, decay, hold = the machine's sustain frames, release; sustain
level ½), so it ends on its own after `attack + decay + sustain + release` frames with no note-off.

A note byte `0x80 | n` (n in 1–126, the Receptor build's glide flag) SHALL start note n and slide its pitch linearly in
frequency towards the note of the next non-empty byte of the same pattern, searched cyclically from the following
step, at the rate that reaches it after that many steps. The slide SHALL run until the next non-empty step in time or
the note's envelope end. The converter SHALL carry it as a pitch-bend lane on the track (within 0.05 semitones of the
curve, byte 128 outside glides) with the instrument's bend range set to the largest offset needed; a glide whose end is
more than 24 semitones away SHALL play its note shifted by whole semitones so both ends fit.

#### Scenario: Long note over empty steps
- **WHEN** a channel holds a note whose envelope lasts 3 steps, followed by two empty steps
- **THEN** the note plays its whole envelope

#### Scenario: Cut
- **WHEN** the step after a note holds `0x7F` or `0xFF`
- **THEN** the note is silent from that step's frame

#### Scenario: Glide
- **WHEN** step 0 holds `0x80 | 57` and step 2 holds 69
- **THEN** the pitch rises from 220 Hz to 440 Hz linearly in frequency over the two steps (330 Hz half way)

## ADDED Requirements

### Requirement: Receptor reference
`tools/elevated-parity/build-ref.sh receptor` SHALL unpack the intro's exe (`crinkler-unpack.py`), carve its
`music.asm`/`music.h` (`carve.py`, checked against the build's code), and render the reference with the intro's own
`generateMusic` (`dump-ref.c` under Wine, x87 at 24 bits; `--solo k` as `render-ref.c`). The exe, dump, carved files and
WAVs SHALL stay outside the repo. `parity.ts` SHALL accept `receptor` and judge it with the Elevated tolerances.

#### Scenario: Acceptance
- **WHEN** `parity.ts --capture receptor` runs with the exe present
- **THEN** the converted song passes against the reference and against the capture `0UOpWfX6gw8`
