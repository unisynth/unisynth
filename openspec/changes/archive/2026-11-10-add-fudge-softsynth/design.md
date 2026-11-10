# Design

## Context

- `add-plain-xm-import` delivered the following, and fixed the engine contract (its design D3, D4):
  - the `xm-gen/1` map and `tools/xm-gen/maps/fudge-asc.json`;
  - in `crates/synth-convert/src/xmgen.rs`: the `Engine` trait, `tone_events` and `GenSource`. `engines()` is empty
    outside tests;
  - the note byte contract: start frame `o::TRACKING`×4096 + `o::PM_OUT`×16, backward pass `o::FEEDBACK` ≥ 128.
- Fudge's two generators, from the bit-exact local port (`$ZENEKK_DIR/re/fudge_synth.py`, never committed). Each
  outputs signed 16-bit values per frame `i`:
  - **Tone** (11 samples):
    - modulator `m = f32(sin(2π·t/mod_period)·depth)` with `t = i·rate`;
    - phase = `m` when FM is off, else `2π·t/(car_period + m + f32(t)·1e-12)`;
    - `v = trunc(sin(phase)·f32(vol·255))`;
    - then the two-stage shaper:
      - `|v| < 8160`: the carried value becomes `4v` and `v` blends toward `4v` by `drive`;
      - `|v| > 16320` (after that blend): the carried value becomes the fold-back of `v`;
      - then `v` blends toward the carried value by `shape`;
    - clip ±32385. The fourth argument is unused.
  - **Drum** (11 samples, 256–1556 frames):
    - `freq −= sweep; sweep −= accel` each frame, f32 stores truncating;
    - `s = f32(sin(i·freq)·f32(vol·255))`;
    - the same shaper family in f32;
    - an envelope `e = clamp(env0 − (i/n)·(env0 − env1), 0, 1)` applied linearly, as `1 − sin(π/2·(1−e))` or as
      `cos(π/2·(1−e))`.
  - **The carried shaper value** (`carry` in the map) is never audible. Frame 0 of both generators is `sin 0 = 0`,
    which falls in the `< 8160` branch and sets the value before anything reads it.
- The Fudge XM needs every seek case:
  - 1869 `9xx` sample offsets;
  - ping-pong loops on tones 4, 5, 6, 17 and 23 (up to 65 536 frames) and on drums 19 and 24 (256 frames).
- **Constraint:** `unisynth` is `no_std` with no heap (`lib.rs`). A slot cannot hold a rendered buffer, and every voice
  state is fixed-size.
- **Storage:** a value is an f32 in 0..1, stored exactly in the chunk. The type choice is `Choice(25)`; odd positions
  other than 1, 3 and 5 are free (`osc_type::is_gap`). Module ids end at 74.
- **The XM flow writes old-layout instrument chunks**, which `SongM::to_v3` migrates when the blob is written. The old
  layout cannot hold positions 7 and 9.

## Goals / Non-Goals

**Goals:**
- Fudge's 22 generated samples played by unisynth slots, with no stored PCM.
- Per-sample parity against the local reference, then the song row and publication.
- A DAW panel where an oscillator's fields follow its type.

**Non-Goals:**
- poespack (`add-poespack-softsynth`).
- Bit-exact song output: x87 80-bit intermediates against f64 here. Per-sample targets are set in D6.
- A general "engine plug-in" ABI: each engine is a type with its own frame function.

## Decisions

### D1. Two types at positions 7 and 9, features of their own

| Position | Label | Feature / module | Engine id in maps |
|---|---|---|---|
| 7 | Shaper tone | `us-osc-shaper-tone` (module 75) | `fm-shaper` |
| 9 | Sweep drum | `us-osc-sweep-drum` (module 76) | `sweep-drum` |

- Both features enable `us-osc-clocked`: they share its Catmull-Rom history, its fractional clock and `ClockedLp`.
  Only the frame function differs (add-plain-xm-import D3).
- `is_gap` drops 7 and 9. `needs` and `module` name the new modules. Loading either type in a build without its
  feature fails with `UnsupportedModule`.
- Existing goldens and the TBL identity guard stay byte-identical, because no existing type changes.

### D2. Engine clock from the slot pitch (256 frames per cycle)

- An engine slot steps `256·hz/rate` engine frames per output frame. `hz` is the slot frequency from the note,
  `o::COARSE`, `o::FINE`, `o::OCTAVE`, `o::RATIO` and the voice's bend and transpose, as for every tracking slot.
  The slot always tracks the note, because `o::TRACKING` is its position byte.
- Why the clock is not a shape value, as `shape::CLK_CLOCK` is:
  1. it frees a shape value, so the drum's nine values fit (D3);
  2. an XM slide bends the whole sample, so speed follows pitch exactly as playback does;
  3. a 256-frame drum loop sounds at the note's own pitch.
- The converter writes a clock `C` (Hz) as note 60 plus `12·log2(C/(256·261.6256))` semitones, split into octave
  (±4), coarse (±24) and fine (±1), with ratio 1 and ratio fine 0. That covers `C` from 983 Hz to 4.55 MHz (±73
  semitones). The relative error is about 1e-7.
- An engine kind always takes the note path. The voice's `FIXED_HZ` branch (`choice(TRACKING) == 1`) is skipped for
  it, as for the clocked kind, because a position byte of 128 or more would otherwise switch it to a fixed frequency.
- Pitch modulation (the mod matrix's pitch targets) and unison detune scale the clock. That is intended: they are the
  slot's pitch, and on a sample-like engine pitch is speed. The converter routes neither. Start spread and the start
  phase do not apply to an engine slot.
- Unused fields: `o::FIXED_HZ` (the slot always tracks the note), `o::FEEDBACK` (its value is the backward byte) and
  `o::PM_OUT` (the position byte). An engine slot sends 0 into the next slot, as a clocked slot does.

### D3. Shape values on exact power-of-two laws

The value `x` of each argument is stored as `p = x / scale`, with `scale` a power of two, so `p·scale` gives back
exactly the f32 the original pushed. This was verified on all 22 entries. The converter rejects a map whose value is
out of range or does not round-trip exactly, and the error names the instrument, sample and argument. The one
exception is the drum's slope: its affine law cannot round-trip exactly, so it must come back within half the law's
step, 2⁻²⁸ per frame. That moves the envelope by at most 6e-6 over 1556 frames.
- **Integer flags** (FM on/off, the envelope curve) are read as the integers the original compares, never through
  the float decode (a raw `1` is the denormal 1.4e-45 as a float). They are written as `Choice` values 0/1 and
  0/0.5/1.

| s | Shaper tone | law | Sweep drum | law |
|---|---|---|---|---|
| 0 | carrier period (time units) | `1024p` | volume | `512p` |
| 1 | modulator period | `1024p` | start frequency (rad/frame) | `4p` |
| 2 | modulation depth (rad) | `64p` | sweep (rad/frame per frame) | `p` |
| 3 | volume | `256p` | sweep growth (= −accel) | `p/64` |
| 4 | drive | `p` | drive | `p` |
| 5 | shape | `p` | shape | `p` |
| 6 | FM | `Choice(2)` off/on | envelope start | `p` |
| 7 | rate (time units per frame) | `16p` | envelope slope per frame | `(2p − 1)/16` |
| 8 | – | – | envelope curve | `Choice(3)` linear, 1 − sin, cos |

- **Envelope slope:** the drum's envelope end and call length combine into a single slope, `(env0 − env1)/n`. The
  slope is signed (drum 16's envelope rises). The stored slope and `i·slope` instead of `(i/n)·span` together move the
  envelope by at most 5e-6 of full scale (measured, drum 8), about −106 dB. The D6 parity is the real gate.
- **Sweep growth** is stored as a magnitude: all of Fudge's `accel` values are ≤ 0. A positive `accel` is rejected.
- **The tone's unused fourth argument** is dropped. A test on the local reference shows that changing it does not
  change the output.

### D4. Seeking in constant state

Each slot keeps fixed-size state: the frame index `i` (i64), the direction, the carried shaper value, the drum's
`freq` and `sweep` at `i`, and the drum's 64-frame backward block.

- **Tone:** `v_raw(i)`, the value before the shaper, is closed-form in `i`. The carried value at `i` is the value set
  by the last frame `j ≤ i` that set one: either `|v_raw(j)| < 8160`, or the blended value past 16320.
  - Forward steps carry it as the original does.
  - A seek or backward step scans back from `i` to that frame. The scan is short, because the keep band
    `|v| ∈ [8160, 16320]` covers about 5 % of a period per flank. It always ends at frame 0; the song's longest scan
    is measured in task 3.6.
- **Drum:** a seek replays the recurrence from frame 0 to the start frame, at most 1556 steps for this intro.
  - A backward pass fills a fixed 64-frame block in the slot: an exact forward replay from frame 0 up to the block's
    first frame, then 64 frames. It serves the block from the top down and refills below it.
  - The cost is at most `i/64 + 1` steps per frame amortised (about 25 for this intro), and the frames are exactly the
    forward ones.
  - Rejected: inverting the recurrence. The two truncations toward zero bias each round trip, so the error builds up
    to about 650 ulp of `freq` and 8 LSB of output over a 256-frame pass (measured, drum 11). Also rejected: replaying
    from 0 on every frame, which costs up to 1556 steps per frame inside a ping-pong loop.
- **At a non-legato note-on** the slot reads the start frame from `o::TRACKING`×4096 + `o::PM_OUT`×16 and the backward
  pass from `o::FEEDBACK` ≥ 128 (bytes `round(255p)`). It seeks, then clears the resampler history. A legato note-on
  keeps the position.
- **Going backward past frame 0** holds frame 0. The converter never asks for it: ping-pong runs end at the loop
  start.
- **Rejected:**
  - a per-voice buffer (no heap; a fixed buffer of more than 1M frames in every slot would be absurd);
  - the last-seek cache of add-plain-xm-import D4 (the costs below make it unnecessary).
- **Budget** (`performance-optimization`, task 3.6): the worst note-on is under 50 µs. It is measured over every
  16th start frame of all 22 samples, forwards and backwards, first frame included (`note_on_budget`, release build).
  - The first measurement was 200–272 µs: the drum's replay computed a sine per frame (about 150 ns).
  - The fix: a seek replays only the sweep (two f32 steps a frame), keeping the last 64 frequencies in a
    stack-local ring, then scans back over them for the carried value. If the scan runs past the ring, it replays in
    full.
  - Now the worst is 36–44 µs: a drum's backward note-on (its 64-frame block) and tone 17's backward scan.
  - The last-seek cache stays out.

### D5. Converter: the engines and the v3 slot

- `xmgen/fudge.rs` holds the two `Engine` implementations, registered in `engines()`:
  - they decode the raw arguments (float bit patterns, plus integer flags such as FM and the envelope curve);
  - they check the ranges and the exact round-trip of D3;
  - they ignore `carry` and the tone's unused fourth argument;
  - they return the clock split of D2.
- **The old layout cannot hold the new types** (old `osc_type::ALL` is 7). The engine's old-layout instrument is
  complete except for its type:
  - it uses the clocked type as a stand-in;
  - its shape values are the engine's;
  - its pitch fields hold the clock split;
  - its byte lanes go to `o::TRACKING`/`o::PM_OUT`/`o::FEEDBACK`.

  Migration copies the `TRACKING..SHAPE+SHAPES` fields and their lanes verbatim (`migrate::lane`), so after
  `SongM::to_v3` only `o::TYPE` is wrong.
- `Source::v3_type(key) -> Option<usize>` defaults to `None`. When a source returns a type, `xm::convert` records it
  for the track's instrument device. After the song is assembled, it runs `to_v3` (idempotent: the blob write leaves
  v3 parts alone), decodes those devices' v3 chunks, sets `o::TYPE` of slot 1 and re-encodes them.
- A song without such tracks takes no new step. What stays byte-identical for TBL is the migrated output, which every
  song gets at write time anyway.
- Rejected: migrating inside the track build. `levels()` writes old-index values and lanes after `src.lanes()`, so a
  v3 array there would be overwritten or mixed.

### D6. Parity

1. **Frame level** (Rust, `crates/unisynth`): the engine frame functions at native clock, compared with each
   generated sample in `fudge-asc.reference.xm`. The test skips when `$ZENEKK_DIR` is absent.
   - **Target:** at most 1 LSB on at least 99.9 % of frames per sample, and no frame off by more than 64 LSB (an x87
     truncation edge in the shaper can flip a branch).
   - The backward pass from the end of each ping-pong loop equals the forward render reversed: exactly for the drum
     (block replay), within the same bounds for the tone.
   - Each seek equals a forward render from 0.
2. **Voice level:** at a clock equal to the output rate, a slot's output equals the frame function delayed by 2
   frames (Catmull-Rom at fraction 0, low-pass bypassed).
3. **Song level:** the row in `tools/unisynth-parity/plain-xm.ts` turns from pending to judged, with the shared
   metrics against the reference XM.

Unit tests that run without the reference: the shaper thresholds, the three envelope curves, the carried value
reset at frame 0, the law round-trips, the type and module gating, and the seek and backward-step identities on
synthetic parameters.

### D7. DAW: fields follow the type

- The oscillator panel already relabels `s0`–`s8` by type (`OSC_SHAPES` in `Device.tsx`). This change makes the rest
  of a slot follow its type too:
  - **Labels** for kinds 16 (Shaper tone) and 17 (Sweep drum), from `oscType` (labels "Shaper tone" and "Sweep drum").
  - **Value text in engine units** (`fmtVal`), for example "128 per", "0.11 rad", "×4 sweep", "5.1e−4" (slope per frame),
    "cos".
  - **Common fields:** an engine slot shows only those it reads: coarse, fine, octave, ratio, gain, mix, pan. Tracking,
    fixed Hz, PM out and feedback are hidden: they are the converter's position bytes, and showing them as pitch knobs
    would mislead. A per-type list `OSC_COMMON_BY_TYPE` falls back to `OSC_COMMON`, so the other types are unchanged.
  - **Defaults on a type change:** when a slot's type changes to an engine type, `setParam` writes that type's default
    shape values: Fudge's instrument 1 (tone) or 8 (drum), decoded through D3. The type change and its defaults are
    one undo step. Changing to any other type keeps today's behaviour.
  - **Screen:** a drawing of the first frames the engine generates, from a TS mirror of the frame functions. The tests
    pin it to engine fixtures, as `pointCycle` is pinned.

## Risks / Trade-offs

- **Frozen once published:** positions, laws and the clock rule. Mitigation: `doubt-driven-review` on D1–D3 before the
  first song is written (task 1.2).
- **Scan-back length on unusual parameters:** a sine that stays between 8160 and 16320 for long stretches would scan
  far back. Mitigation: the scan always ends at frame 0, so it is correct in any case. Its cost is measured on the
  song (D4), and a pathological patch costs CPU, not correctness.
- **The drum's block replay cost** grows with the start frame: `i/64` steps per frame. Fudge's drums are at most 1556
  frames long. A future engine with long recurrences measures it again, and can grow the block.
- **f64 against x87 80-bit:** branch flips at truncation edges. Mitigation: the targets in D6; drift that breaks a
  band is debugged per engine.
- **The type patched after migration** (D5). Mitigation: only `o::TYPE` changes, after the song's own `to_v3`. A test
  checks that the converted song loads, that its byte lanes address the v3 fields of slot 1, and that the stand-in's
  shape values come through unchanged.
