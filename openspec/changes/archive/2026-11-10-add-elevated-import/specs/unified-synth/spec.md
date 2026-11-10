# Spec Delta

## ADDED Requirements

### Requirement: Pair oscillator slot type
A `pair` oscillator slot SHALL play two naive copies of one waveform (sine, square or saw). They are read from two
phases that advance by `inc·(2 − d)` and `inc·d` per sample, where `inc` is the slot's increment and `d` the detune
ratio. Both phases start at 0 at note-on and advance before the first read. Each copy SHALL add the slot's phase shift
`s` and fold with `x = 2·(t − round(t))`. The waveforms are: sine `sin(π·x)`, square `+1` for `x > 0`, else `−1`, saw
`x`. The slot's output SHALL be the sum of both copies, negated when its polarity is set (so an added, negated pair
subtracts from the running sum). The slot is built with `us-osc-pair`.

#### Scenario: Unison at d = 1
- **WHEN** a pair slot plays a saw with `d = 1` and `s = 0`
- **THEN** its output is twice a single naive saw at the note frequency

#### Scenario: Detuned pair
- **WHEN** a pair slot plays a sine at 440 Hz with `d = 0.99`
- **THEN** the spectrum peaks at 435.6 Hz and 444.4 Hz

#### Scenario: Polarity subtracts
- **WHEN** slot 1 and slot 2 are identical pair slots, both added, and slot 2 has its polarity set
- **THEN** the voice output is silent (below −120 dBFS)

### Requirement: Pair type storage
The `pair` type SHALL be stored at a free position of the expanded oscillator-type choice, so stored values of existing
types keep their meaning. Waveform, phase shift, detune and polarity SHALL use the slot's shape fields. A patch using
the type in a build without `us-osc-pair` SHALL fail at load naming the module.

#### Scenario: Existing songs unchanged
- **WHEN** the existing golden songs render in a build with `us-osc-pair`
- **THEN** they are byte-identical to their goldens

#### Scenario: Missing module
- **WHEN** a patch with a pair slot loads without `us-osc-pair`
- **THEN** the load fails with `UnsupportedModule` naming the pair oscillator

### Requirement: Pitch drop
A voice SHALL offer a pitch drop with a base frequency `b` (Hz) and a per-sample factor `k`, as appended parameters.
With the drop on (`k < 1`), the voice frequency at sample `n` after note-on (`n = 0` first) SHALL be
`b + (f − b)·kⁿ⁺¹`, where `f` is the note frequency. It is evaluated at the control points, and the increment is ramped
linearly between them (relative error below 1e-3). The code is built with `us-voice-drop`. With `k = 1` the pitch
behaves as before, and the module is not required.

#### Scenario: Kick drop
- **WHEN** a note at 110 Hz plays with `b = 40 Hz` and `k = 0.9995`
- **THEN** the instantaneous frequency is within 0.5 Hz of 110 Hz at note-on and within 0.1 Hz of 40 Hz after 1 s

#### Scenario: Off
- **WHEN** `k = 1`
- **THEN** the voice plays the note frequency, and the drop module is not required at load

### Requirement: Linear timed envelope
Every envelope SHALL have two appended parameters: `linear` (off, on) and `hold` (a time). A linear envelope SHALL run
on its own:
1. attack from 0 to 1
2. decay to the sustain level
3. hold the sustain level for the hold time
4. release to 0

Each stage is linear in level, and the stage times are `5000·p²` ms with no minimum. A note-off on a linear envelope
SHALL end it at once (level 0, finished), so a note-off cuts a voice whose envelope 1 is linear. When envelope 1 is
linear, the voice SHALL end its control segment at the envelope's next stage corner, so the corners are frame-exact
(a 3-frame attack keeps its corner). The code is
built with `us-env-linear`. With `linear` off, envelopes behave as before.

#### Scenario: Linear stages
- **WHEN** a linear envelope has attack 1000, decay 2000, hold 2000 and release 4000 frames, with sustain ½
- **THEN** the level is within 0.002 of 0.5 at frame 500, 0.75 at frame 2000, 0.5 at frames 3000–5000, 0.25 at frame
  7000, and 0 from frame 9000, without any note-off

#### Scenario: Note-off cuts
- **WHEN** a voice whose envelope 1 is linear gets a note-off during its hold stage
- **THEN** its output is silent within 32 frames of the note-off

#### Scenario: Existing songs unchanged
- **WHEN** the existing golden songs render in a build with `us-env-linear`
- **THEN** they are byte-identical to their goldens

### Requirement: Monophonic hard cut
With capacity 1 and the steal setting "oldest", a new note SHALL restart the sounding voice at its own frame, with no
release of the old note.

#### Scenario: Mono cut
- **WHEN** a capacity-1 instrument with steal oldest plays a note with a 1 s release, and a second note comes 0.5 s later
- **THEN** from the second note's frame, the output holds only the second note
