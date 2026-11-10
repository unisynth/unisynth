# Spec Delta

## ADDED Requirements

### Requirement: Engine oscillator types
The oscillator-type choice SHALL hold two engine types at free positions:
- position 7, "Shaper tone", built with `us-osc-shaper-tone`;
- position 9, "Sweep drum", built with `us-osc-sweep-drum`.

Stored values of existing types keep their meaning. An engine slot SHALL always take the note path, never the fixed
frequency, whatever its `o::TRACKING` byte. An engine slot SHALL step `256·hz/rate` engine frames per
output frame. `hz` is the slot frequency (note, coarse, fine, octave, ratio, bend and transpose); the slot always
tracks the note. The engine frames SHALL be resampled with the clocked slot's Catmull-Rom and reconstruction
low-pass. An engine slot SHALL send 0 into the next slot's phase. A patch using an engine type in a build without
its feature SHALL fail at load, naming the module.

#### Scenario: Existing songs unchanged
- **WHEN** the existing golden songs render in a build with both engine features
- **THEN** they are byte-identical to their goldens

#### Scenario: Missing module
- **WHEN** a patch with a Sweep drum slot loads without `us-osc-sweep-drum`
- **THEN** the load fails with `UnsupportedModule` naming the sweep drum oscillator

#### Scenario: Speed follows pitch
- **WHEN** a Sweep drum slot plays note 72 instead of note 60, all else equal
- **THEN** it plays the same frames in half the time

### Requirement: Shaper tone
The engine frame at index `i` SHALL be computed in this order:
1. Time `t = i·rate`.
2. Modulator `m = f32(sin(2π·t/mod_period)·depth)`.
3. Phase `m` with FM off, or `2π·t/(car_period + m + f32(t)·10⁻¹²)` with FM on.
4. `v = trunc(sin(phase)·A)`, where `A = f32(vol·255)`.
5. If `|v| < 8160`: the carried value becomes `4v`, and `v = trunc((1 − drive)·v + drive·4v)`.
6. If, after step 5, `v > 16320`: the carried value becomes `(16320 − (v − 16320)/4)·127/80`, with each division
   truncating toward zero. If `v < −16320`: it becomes the mirror of that value.
7. `v = trunc((1 − shape)·v + shape·carried)`.
8. Clip `v` to ±32385; the output is `v/32768`.

The shape values SHALL decode on the laws:
- `s0` carrier period `1024p`
- `s1` modulator period `1024p`
- `s2` depth `64p`
- `s3` volume `256p`
- `s4` drive `p`
- `s5` shape `p`
- `s6` FM `Choice(2)`
- `s7` rate `16p`

#### Scenario: Frame 0 resets the carried value
- **WHEN** a Shaper tone slot starts with any carried value
- **THEN** frame 0 outputs 0, and frame 1 onwards equals a start from a carried value of 0

#### Scenario: Drive below the threshold
- **WHEN** `drive` is 0.5, `shape` is 0, and `v` before the shaper is 1000
- **THEN** the frame outputs `trunc(0.5·1000 + 0.5·4000)` = 2500 (/32768)

### Requirement: Sweep drum
The engine SHALL keep the f32 values `freq` (initially the start frequency) and `sweep`. At each frame `i` (from 0),
truncating to f32 at each step:
1. `freq = f32(freq − sweep)`, then `sweep = f32(sweep + growth)`.
2. `s = f32(sin(i·freq)·A)`, where `A = f32(vol·255)`.
3. The Shaper tone's shaper, in f32, with the fold factor `127·f32(0.0125)`.
4. Clip `s` to ±32385.
5. Envelope `e = clamp(start − i·slope, 0, 1)`, applied linearly, as `1 − sin(π/2·(1 − e))`, or as
   `cos(π/2·(1 − e))`.
6. The output is `trunc(f32(s·env))/32768`, where `env` is the applied envelope from step 5.

The shape values SHALL decode on the laws:
- `s0` volume `512p`
- `s1` start frequency `4p` rad/frame
- `s2` sweep `p`
- `s3` growth `p/64`
- `s4` drive `p`
- `s5` shape `p`
- `s6` envelope start `p`
- `s7` slope `(2p − 1)/16`
- `s8` curve `Choice(3)`

#### Scenario: Envelope curves
- **WHEN** a drum with volume 0 is replaced by a constant input of 1000 at envelope value 0.5
- **THEN** the output is 500 (linear), `1000·(1 − sin(π/4))` (1 − sin) and `1000·cos(π/4)` (cos), each truncated

#### Scenario: Rising envelope
- **WHEN** the envelope start is 0 and the slope is negative
- **THEN** the envelope rises from 0 and holds at 1

### Requirement: Engine start position and backward pass
At a non-legato note-on, an engine slot SHALL read:
- its start frame `round(255·TRACKING)·4096 + round(255·PM_OUT)·16`;
- a backward pass when `round(255·FEEDBACK)` ≥ 128.

The slot SHALL seek to the start frame and clear its resampler history. A backward pass SHALL produce the frames in
descending index order, from the start frame down, and hold frame 0. A legato note-on SHALL keep the position. A
seek SHALL give the same frames as a forward run from frame 0:
- the Shaper tone's carried value is recovered by scanning back to the last frame that set it;
- the Sweep drum replays its recurrence from frame 0.

A drum's backward pass SHALL be served from a fixed 64-frame block, filled by an exact forward replay. Seeking SHALL
use fixed-size state (no heap).

#### Scenario: Offset equals replay
- **WHEN** a Shaper tone note starts at frame 20 000
- **THEN** its engine frames equal frames 20 000 onward of a run from frame 0

#### Scenario: Backward pass
- **WHEN** a Sweep drum note starts at frame 255 with the backward byte set
- **THEN** its engine frames equal frames 255, 254, … 0 of a forward run exactly, then frame 0 repeats

#### Scenario: Note-on budget
- **WHEN** the converted Alien Sex Clone renders
- **THEN** no engine note-on takes more than 50 µs on the reference machine (release build)
