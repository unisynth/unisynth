# Spec Delta

## ADDED Requirements

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
