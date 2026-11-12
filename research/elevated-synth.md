# Mentor's 4k synthesizer (Elevated, Atrium): behavioural reading

Reference notes for change `add-elevated-import`. They describe what the original engine *does*, in our own words,
from reading its NASM source and measuring its output. The original code is CC BY-NC-SA 3.0 and is not copied here or
anywhere in the repo: no code, comments, tables or identifiers beyond the data-section names a song file uses.

## 1. Provenance

| Item | Where | Notes |
|---|---|---|
| Elevated (RGBA & TBC, Breakpoint 2009 4k winner, [pouet 52938](https://www.pouet.net/prod.php?which=52938)) | public source release, mirrored at `github.com/in4k/rgba_tbc_elevated_source` @ `1832f2b` (from scene.org `rgba_tbc_elevated_source.zip`) | music by Puryx, synth by Mentor (Rune L. H. Stubbe), visuals by iq; licence file: CC BY-NC-SA 3.0 |
| Atrium (TBC & Loonies, Breakpoint 2008 4k, [pouet 50063](https://www.pouet.net/prod.php?which=50063)) | `~/Downloads/zenekk/TBC - Atrium/` (`synth.asm`, `music.asm`, `music.h`, `hack.exe`) | the `synth.asm` there carries Elevated's header; `hack.exe` is a debug build of a third-party rebuild (its PDB path names `atrium`), not used |
| YouTube captures (Annikras HQ) | `tools/elevated-parity/captures.sh` → `~/Downloads/64k-tools/elevated-captures` | elevated `jB0vBmiTr6o` (215.5 s), atrium `jlRoOqRs0bs` (218.8 s) |

Everything stays local. The reference build (§9) copies the sources into `/tmp/elevated-ref`.

## 2. Song file

`music.asm` is generated NASM data. A song uses four labels:

| Label | Content |
|---|---|
| `_tempo_data` | `dd` samples per step, `db` a row count, `dd −1` |
| `_pattern_data` | 16 note bytes per pattern |
| `_sequence_data` | for each synth machine in tree order, `NUM_ROWS` pattern indices |
| `_machine_tree` | the machine program (§3) |

`music.h` (shared between C and NASM through a preprocessor trick) holds `NUM_ROWS`, `MAX_NOTE_SAMPLES` (= samples per
step), `TOTAL_SAMPLES` (song length rounded up to 65 536), `MAX_DELAY_SAMPLES` (65 536) and `MAX_STACK_HEIGHT`. The
synth strides the sequence by **`NUM_ROWS` from `music.h`**. `_tempo_data`'s row byte is not used for that and can
differ: Elevated has 113 there and 114 in `music.h`. The sequence can carry padding: Atrium has 904 bytes for
8 × 112.

| | Atrium | Elevated |
|---|---|---|
| step (frames) | 5410 | 5210 |
| rows × 16 steps | 112 | 114 |
| song length | 9 694 720 frames (219.8 s) | 9 503 040 frames (215.5 s) |
| synth machines | 8 | 12 |
| patterns | 36 | 70 |
| notes / cuts (`0x7F`) | 2930 / 0 | 1509 / 84 |
| machine tree | 1059 bytes | 1398 bytes |

## 3. Machine tree

The tree is a byte program run once over the whole song. The first record is always a synth with no opcode byte.
After each record comes an opcode byte, then the next record. A negative opcode byte ends the program. Machines work
on a stack of stereo float buffers, each as long as the song:
- a synth pushes a new buffer
- an effect transforms the top buffer in place
- a mixer pops the top buffer and mixes it into the one below

The last remaining buffer is converted to 16-bit (§8).

**The opcode table is a property of the build, not the song.** Atrium's synth has 8 machines; Elevated's has the atan
distortion commented out, so its opcode 6 is the allpass:

| Machine | Record bytes | Atrium opcode | Elevated opcode |
|---|---|---|---|
| synth | 72 | 0 | 0 |
| delay | 12 | 1 | 1 |
| filter | 32 | 2 | 2 |
| compressor | 12 | 3 | 3 |
| mixer | 8 | 4 | 4 |
| sine distortion | 8 | 5 | 5 |
| atan distortion | 8 | 6 | — |
| allpass | 12 | 7 | 6 |

Each tree walks to exactly its last byte only under its own table, so a converter can detect the table.

Effects keep running state (filter LFOs, delay positions) inside their parameter records and in a separate zeroed
state block.

## 4. Notes

Synth machine k reads rows `k·NUM_ROWS …` of the sequence. Step s of row r starts at frame `(16r + s)·step`.

- **Pitch:** note byte n (1–126) is MIDI note n. The frequency starts at 8.1758 Hz / 44 100 and is multiplied `2n`
  times by `2^(1/24)`.
- **Note 0** renders nothing, so whatever is already in the buffer stays.
- **Note `0x7F`** renders the note with all envelope steps zero. That writes silence for the instrument's full
  envelope length, which is a cut.
- **Overwrite, not add.** Each note *stores* its samples into the buffer, starting at its step. A note longer than a
  step runs into the following steps, and the next rendered note overwrites it from its own start. So every channel
  is **monophonic with a hard cut**, and a note's tail survives only over empty steps.
- **Phase** starts at 0 for every note.

## 5. Voice

Per sample, in order:

1. **Envelope:** four linear segments with integer frame lengths A, D, S, R and level steps `+1/A`, `−½/D`, `0`,
   `−½/R`. The level is advanced *before* use, starting at 0. So the attack ends at 1, the decay at ½, the sustain
   holds ½ for S frames, and the release ends at 0. A zero-length segment would divide by zero and loop about 2³²
   times; no song has one.
2. **Pitch:** the per-sample increment is `b + (f − b)·k^(n+1)`, where f is the note frequency and b the base, both in
   cycles per sample, k = `freq_exp`, and n = 0 at the first sample. The phase is advanced before it is read. The
   pitch falls exponentially from the note towards the fixed base frequency (kick drums). With k = 1 the pitch is
   constant at f.
3. **Oscillators:** three pairs. Pair j has a waveform (1 sine, 2 square, any other value saw), an op (1 off, 2 add,
   3 subtract, 4 multiply), a phase shift s and a detune d. The two copies read `φ·(2 − d)` and `φ·d`. Each adds s,
   folds to `x = 2·(t − round(t))` in [−1, 1], and gives:
   - sine `sin(πx)`
   - square `+1` for x > 0, else `−1`
   - saw `x`

   The pair is the sum of its two copies. An accumulator starts at 0, and each pair is applied to it by its op:
   - off drops the pair
   - add adds it
   - subtract gives accumulator − pair
   - multiply gives accumulator · pair
4. **Noise:** uniform white noise (a 32-bit LCG, `×16307 + 17`, bits 14–29 as a signed 16-bit value / 32 768, one
   global stream in render order) times the noise mix, added to the accumulator.
5. **Output:** `L = acc·env·volume`, `R = L·stereo`. The stereo factor is a gain ratio, not a pan.

The phase and envelope live in x87 80-bit registers. The phase is never wrapped; it is folded only when read.

## 6. Effects

All effects work per sample on the full stereo buffer.

| Machine | Law |
|---|---|
| filter | Two coupled-form LFOs, `c ← c − r·s; s ← s + r·c`, starting from s = 0 and c = depth (the record's own c is overwritten as it runs). The coefficient is `g = (cutoff + s₁ + s₂)·3.5524e-5`. Per channel, the step is `low += g·band; high = q·(x − band) − low; band += g·high`. The output is low, high or band (type 0/1/2) plus `dry·x`. This is the same step as the shared state-variable filter's input form in `crates/unisynth/src/flt.rs`, run once per sample instead of twice. |
| compressor | Memoryless: `y = sign(x)·(t + (|x| − t)·r + o)` when `|x| ≥ t`, else `y = x`. With `(1, 0, 0)` it is a hard clip at ±1; both songs end on one. |
| sine distortion | `y = b·sin(a·x)` |
| atan distortion | `y = b·atan(x/a)` (`fpatan`): the V2 overdrive arithmetic with `gain1 = 1/a`, `gain2 = b`, `offs = 0` |
| delay | One stereo ring of `len` frames. With `(wL, wR)` read: `yL = xL + g·wL`, `yR = xR + g·wR`; the line stores `(yR, yL)`, so the channels swap on every pass. No dry/wet control. The initial position does not matter, because the ring starts silent. |
| allpass | With `(wL, wR)` read: `vL = xL + g·wR`, `vR = xR + g·wL`; the line stores `(vL, vR)`; the output is `yL = wR − g·vL`, `yR = wL − g·vR`. This is a Schroeder allpass whose feedback crosses the channels (paraunitary, so power is kept). |
| mixer | `below = top·g₀ + below·g₁`, then the top is popped. |

## 7. The songs' settings

Ranges that the instrument encodings must hold (design D2, D7):

| | Atrium | Elevated |
|---|---|---|
| pair waveforms used | sine, square, saw | sine, saw |
| pair detune d (pairs not off) | 1.0 – 1.5 | 0.00049 – 1.5 (Elevated's pads subtract a near-DC pair: d = 0.0073, 0.00049) |
| phase shift | 0 – 0.398 | 0 – 0.406 |
| segment lengths | 3 – 98 304 | 3 – 196 608 |
| pitch drop | one synth (kick): base 50 Hz, k = 0.9993 | one synth (kick): base 50 Hz, k = 0.99908 |
| volume | 0.0625 – 1.875 | 0.0508 – 8 |
| stereo factor | 0.001 – 8 | 0.01 – 9 |
| noise-only synths (all pairs off) | 2 | 3 |
| mixer gains | 0.5 – 30 | 0.203 – 4 |
| filter cutoff (units) / q / LFO depth | 352 – 29 952 / 0.02 – 0.81 / 0 – 7936 | 320 – 22 528 / 0.31 – 1.0 / 0 – 5120 |
| delay / allpass lengths | 813 – 32 460 / — | 15 630 – 31 260 / 52 – 4314 |

No gain, stereo factor or volume is negative in either song.

## 8. Output

The final buffer is scaled by 32 767 and stored with `fistp` to 16-bit. A value past full scale stores −32 768 (x87
integer indefinite) instead of clipping. Both songs end on a hard-clip compressor at ±1, so this never triggers in them.

## 9. Reference renders and capture check

`tools/elevated-parity/build-ref.sh` assembles the original synth with each song (`nasm -f win32`), links our MIT
harness `render-ref.c` and runs it under Wine in a container. It takes about 8 s per render on the dev machine.
`SOLO=1` adds one render per synth machine.

| Render | Frames | Loudness | Solo renders |
|---|---|---|---|
| atrium.wav | 9 699 328 | −9.0 LUFS (capture −9.2) | 8, −10.4 … −26.8 LUFS |
| elevated.wav | 9 568 256 | −10.2 LUFS (capture −10.5) | 12, −13.7 … −26.3 LUFS |

Capture check (`tools/elevated-parity/capture.ts`, reference render vs YouTube capture, 2026-11-10):

| Song | Octave bands 63 Hz – 8 kHz (dB) | Timing |
|---|---|---|
| Atrium | −0.04 0.03 0.34 0.24 −0.18 −0.01 −0.12 −0.06 | 5/7 windows ≥ 0.5, all at lag −128 ± 64 frames: PASS |
| Elevated | −0.06 −0.04 −0.04 −0.05 −0.00 −0.08 −0.11 −0.26 | 5/7 windows ≥ 0.5, all at lag 3328 ± 64 frames: PASS |

The local `music.asm` files are the released songs. The Tunefish 3 timing rule (≥ 90 % of windows ≥ 0.5) fails on
both: the pad openings and breaks are onset-poor and correlate weakly at random lags. Hence the consistency rule in the
spec (user decision 2026-11-10).

## 10. Parity of the converted songs

`bun tools/elevated-parity/parity.ts --solo --capture` (2026-11-10). The converted song is rendered by `unisynth` and
compared with the original code's render. Tolerances are those of `tools/unisynth-parity`'s song parity: loudness 1 dB,
octave bands 3 dB above a 24 dB floor, timing by onset-strength alignment (correlation ≥ 0.5). Peak-picked onsets are
not judged, as there: the picker keeps different hits of the noise hats, and only 2–3 hits of a pad.

| render | result | loudness Δ dB | worst band Δ dB | alignment corr | capture | notes |
|---|---|---:|---:|---|---|---|
| atrium | PASS | 0.14 | 1.08 @ 1000 | 0.99 | PASS |  |
| atrium/solo0 | PASS | 0.00 | -2.28 @ 250 | 0.93 |  |  |
| atrium/solo1 | FAIL | 0.23 | 1.48 @ 500 | 0.32 |  |  |
| atrium/solo2 | PASS | -0.28 | 3.78 @ 63 | 1.00 |  |  |
| atrium/solo3 | PASS | 0.18 | 0.56 @ 4000 | 0.98 |  |  |
| atrium/solo4 | PASS | 0.06 | 1.49 @ 63 | 0.93 |  |  |
| atrium/solo5 | PASS | 0.02 | 2.30 @ 63 | 0.94 |  |  |
| atrium/solo6 | PASS | 1.00 | 1.45 @ 1000 | 0.98 |  |  |
| atrium/solo7 | PASS | 0.04 | -2.11 @ 16000 | 1.00 |  |  |
| elevated | PASS | -0.03 | -0.51 @ 125 | 0.87 | PASS |  |
| elevated/solo0 | PASS | -0.21 | 1.18 @ 63 | 1.00 |  |  |
| elevated/solo1 | PASS | 0.02 | 3.10 @ 63 | 0.93 |  |  |
| elevated/solo2 | PASS | 0.08 | -1.14 @ 125 | 0.94 |  |  |
| elevated/solo3 | PASS | 0.07 | -1.14 @ 125 | 0.93 |  |  |
| elevated/solo4 | PASS | 0.02 | -1.17 @ 8000 | 1.00 |  |  |
| elevated/solo5 | PASS | -0.33 | -0.78 @ 125 | 0.56 |  |  |
| elevated/solo6 | FAIL | 0.14 | 1.01 @ 250 | 0.35 |  |  |
| elevated/solo7 | PASS | 0.28 | 1.16 @ 125 | 0.56 |  |  |
| elevated/solo8 | PASS | 0.04 | 1.49 @ 63 | 0.70 |  |  |
| elevated/solo9 | PASS | 0.02 | -0.84 @ 63 | 0.58 |  |  |
| elevated/solo10 | PASS | 0.01 | 1.40 @ 63 | 0.74 |  |  |
| elevated/solo11 | PASS | 0.23 | 0.32 @ 4000 | 0.56 |  |  |

**Both songs pass, and so do their capture checks.** 20 of 22 renders pass. The two that do not are slow pads:
- Atrium synth 1 has a 50 176-frame attack; Elevated synth 6 has an 81 920-frame attack and a 131 072-frame release.
- They have no transients, so onset-strength alignment has nothing to lock on (0.32, 0.35).
- Their levels and bands match: loudness within 0.23 dB, bands within 1.5 dB.
- Their level envelopes correlate at 0.99 and 0.94 with a flat peak around lag 0, so no timing offset.

Ruled out:
- **The filter LFO.** The closed form against the original's f32 coupled-form recurrence differs by at most 1.6 units
  over 9.7 M samples, under 0.1 % of depth.
- **The kick's attack.** Before the stage-corner fix (design D4, task 5.1), a 3-frame attack was smeared over the
  8-frame control window and the kick lost its click: Elevated synth 4 was −5.90 dB at 8 kHz, Atrium synth 7 −3.03 dB.
  Envelope 1 now ends its control segment at each linear stage corner, which brings them to −1.17 and −2.11 dB.

Speed: Atrium renders 221.8 s in 6.7 s and Elevated 217.5 s in 8.4 s (`--profile fast`, dev machine), 26–33× real time.

## 11. Size (task 5.2, 2026-11-10)

`research/us-size-probe/songs-size.sh` (compo probe, the song baked as Rust by `synth-convert rust`, exactly its
feature list; xz -9e / squishy 0.1.3). `tools/check-song-features.sh --minimal` on Atrium: every listed feature is
needed, and the i686 core-only build works with exactly that list.

| song | raw B | xz B | squishy B | features |
|---|---:|---:|---:|---|
| atrium | 59 904 | 22 060 | 18 944 | us-noise, us-env-seconds, us-chunk-wide, us-osc-pair, us-voice-drop, us-env-linear, us-fx-eq-svf-lfo, us-fx-distortion-sine, us-fx-distortion-knee, us-fx-delay-pingpong |
| elevated | 53 248 | 22 504 | 19 456 | the same, plus us-fx-delay-allpass |

Marginal code cost of each new module (`marginal.sh`: the all-modules probe against the same build without it):

| feature | raw B | xz B |
|---|---:|---:|
| us-osc-pair | 1 024 | 380 |
| us-voice-drop | 1 024 | 480 |
| us-env-linear | 1 024 | 420 |
| us-fx-eq-svf-lfo | 2 560 | 1 076 |
| us-fx-distortion-atan | 3 072 | 1 184 (it reuses V2's distortion stage whole; neither song uses it) |
| us-fx-distortion-sine | 512 | 116 |
| us-fx-distortion-knee | 512 | 132 |
| us-fx-delay-pingpong | 512 | 140 |
| us-fx-delay-allpass | 512 | 148 |

The swept filter first cost 3 120 xz bytes and the sine 652: `libm`'s f64 `sin`/`asin` and `sinf`. They now use
`fm::sin` and a series for `asin` (r/2 ≤ 0.005), at 1 076 and 116. The song builds shrank by about 3 KB.

**Cost to songs that use none of it.** The seven reference songs grow by 84–172 xz bytes. Squishy is unchanged for six
of them; engage moves one 512-byte step. The new state is gated by its features: the pair state is zero-size without
`us-osc-pair`, and `Move::Drop`, the `EnvP` fields and the off-grid tick are compiled only with their modules. What is
left is shared: the 10 appended parameters (331-value arrays, `DEFAULTS`), the load checks that refuse the new fields in
builds without them, and the effect-mode position lookup.

| song | xz before | xz now | Δ |
|---|---:|---:|---:|
| global-groove | 16 552 | 16 688 | +136 |
| elysian | 54 224 | 54 332 | +108 |
| engage | 48 360 | 48 532 | +172 |
| eladamri | 39 728 | 39 864 | +136 |
| no-remorse | 28 796 | 28 880 | +84 |
| tf4testsong | 26 964 | 27 084 | +120 |
| turtles | 37 400 | 37 544 | +144 |

## 12. Receptor: the song carved from its exe (change add-receptor-import, 2026-11-11)

**Receptor** (TBC, NVScene 2008 4k, 2nd; [demozoo 31385](https://demozoo.org/productions/31385/),
[pouet 51449](https://www.pouet.net/prod.php?which=51449); code Mentor, music Puryx). The release
(`files.scene.org/get/parties/2008/nvscene08/4k/tbc_-_receptor.zip`) has five Crinkler-packed resolution builds and no
source. Capture: Annikras `0UOpWfX6gw8` (191.8 s).

**Unpacking.** `tools/elevated-parity/crinkler-unpack.py` emulates the exe's own decompressor (unicorn) until it jumps to
0x420000 and dumps from 0x410000: 11.7 KB of code and data. Crinkler keeps its hash tables inside the section's
~270 MB virtual size, so the emulation maps all of it and takes about 3.5 minutes.

**The engine.** Instruction-by-instruction against Atrium's build:

| part | against Atrium |
|---|---|
| data block (note start and step, 32 767, 1/32 768, envelope steps) | the same; the cutoff scale is the full float `0x38156592` (Atrium truncates it to `0x38150000`) |
| machine table (8 machines at 0x421548, atan included) | the same |
| delay, filter, compressor, mixer, sine, atan, allpass, oscillator, noise LCG | identical instructions |
| tree walker | identical but one `cdq` for `xor edx, edx` |
| synth machine | glide flag, gate cut, per-row counters |

The synth machine's changes:
- **Glide.** A note byte with bit 7 starts the note and searches the following steps of its own pattern, cyclically,
  for the next non-empty byte. That byte's note (127 for a cut) is the target, and d is the number of steps. The
  frequency increment gains `(f_target − f_note)/(d·step)` per sample, before the pitch factor. With a pitch factor of 1
  that is a linear slide in frequency. The slide only stops when the next rendered note overwrites the note.
- **Cut.** `0x7F`/`0xFF` set a gate multiplier to 0 for the note's length; the envelope is the normal one. The output
  is the same as Atrium's zero envelope.
- **Counters.** Two dwords per row (frames since the last note-on, note-ons so far) carried row to row. Nothing in the
  synth reads them; the visuals presumably do.

**The song.** At 0x422154 the tree is 2484 bytes: 16 synths, 27 filters, 9 atan, 3 sine, 8 allpass, 5 delays, 15 mixers and
1 compressor. 4010 frames per step (165 BPM), 133 rows, 42 patterns, 193.5 s (8 533 280 frames; `TOTAL_SAMPLES`
0x830000). There are 143 glide notes on six synths (14: 62, 5: 30, 15: 21, 8: 18, 9: 10, 10: 2); all six have a pitch
factor of 1 and a base of 0. Intervals are −24, −12, −5, +12, +24 and +25 semitones, over 2–12 steps. Every glide's
in-pattern target is the note that actually follows it, at exactly that distance. There are 21 cuts. `carve.py receptor`
writes `music.asm`/`music.h`; the intro has no `_tempo_data`, so the carve writes one from the code's constants.

**Reference.** `dump-ref.c` runs the intro's `generateMusic` (0x4206CD) on the dump under Wine. The intro calls it right
after `IDirect3D9::CreateDevice` with behaviour flags 0x20 (no `FPU_PRESERVE`), so the x87 runs at 24 bits; the harness
does the same (`--fpu`). Rendering at 24 and at 53 bits gives the same metrics against the capture: octave bands
within 0.36 dB, all 6 windows aligned (corr ≥ 0.87, lag 64 frames). Synth 5's solo agrees between the two to 0.1 dB.

**Parity** (`parity.ts --solo --capture receptor`, table in `tools/elevated-parity/report.md`):
- The song passes: loudness +0.22 dB, worst band 0.86 dB at 250 Hz, alignment 0.94. The capture check passes too:
  6/6 windows, bands within 0.51 dB.
- 12 of 16 solos pass. The four that fail are slow pads that fail only onset alignment, with loudness within 0.64 dB
  and bands within 2.31 dB:

| solo | patch | alignment corr / lag | level-envelope corr |
|---|---|---|---|
| 6, 7 | attack 98 304 frames | 0.31 / 0.48 | 0.88 / 0.85 |
| 8 | sustain 50 176 frames | 0.50 | 0.92 |
| 14 | attack 3072 frames | 0.55 at lag −128 frames | 0.98 |

  Solo 14 shows the same lag and correlation with every glide flag cleared, in both the dump and the song. Solo 10
  (also attack 3072 frames) is borderline at 0.49 (level-envelope corr 0.99).

Ruled out for solos 6/7: synth 6's pair detune 2.002 (clamped to the encoding's 2, so copy A is a constant instead of a
−0.002·φ sine). Mirroring it to 1.998, which keeps a slow sine, leaves the correlation where it was (0.30, 0.48).

**Found and fixed.** In the first glide lane, a glide that starts at byte 128 added no anchor point at its note, so the
player interpolated the ramp from the previous point, often seconds earlier, and bent the notes before every glide.
Solo 5 was −5.8 dB at 63 Hz. After the fix it is −1.85 dB; with all glide flags cleared it is −1.5 dB, the patch's
baseline. The test is `notes_before_a_glide_keep_their_pitch`.
