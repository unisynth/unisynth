# Spec Delta

## ADDED Requirements

### Requirement: SVF LFO EQ mode
EQ kind 67 SHALL add the mode `svf-lfo` with the parameters cutoff, resonance, LFO 1 rate, LFO 1 depth, LFO 2 rate,
LFO 2 depth, dry and output (low, high, band). For each sample, both channels SHALL use the coefficient
`g = (cutoff + l₁ + l₂)·c` (the cutoff and the LFO depths are in the original's units, `c` its scale) and run these steps
in order:

1. `low += g·band`
2. `high = q·(x − band) − low`
3. `band += g·high`

The output SHALL be the chosen state plus `dry·x`. The step SHALL be the shared state-variable filter's input-form step
(`flt::run`), run once per sample (a one-step form, not the voice slots' two half-steps). Each LFO `l` SHALL follow the original's coupled-form recurrence per sample: `c ← c − r·s`, then
`s ← s + r·c`, with `l = s`, starting from `s = 0` and `c = depth`, where `r` is the LFO's rate. It SHALL be computed
from the absolute song frame in closed form, so a render started mid-song equals the same span of a full render within
1e-4. The mode is built with `us-fx-eq-svf-lfo`.

#### Scenario: Static low-pass
- **WHEN** both LFO depths are 0, output is low and dry is 0
- **THEN** the response is low-pass with passband gain `q` (the input form scales the input), and the resonant peak
  above the passband rises as `q` falls

#### Scenario: Seek
- **WHEN** an input that is silent until frame 441 000 is rendered from frame 0, and again from frame 441 000
- **THEN** the two renders agree from frame 441 000 within 1e-4 (the LFOs stand at the same song time)

### Requirement: Atan, sine and knee distortion modes
Distortion kind 65 SHALL add the memoryless modes:
- `atan` (a, b): `y = b·atan(x/a)`, computed by the V2 overdrive arithmetic with the exact arctangent
  (`us-fx-distortion-atan`)
- `sine` (a, b): `y = b·sin(a·x)` (`us-fx-distortion-sine`)
- `knee` (threshold t, ratio r, offset o): `y = sign(x)·(t + (|x| − t)·r + o)` when `|x| ≥ t`; below the threshold,
  `y = x` (`us-fx-distortion-knee`)

#### Scenario: Knee transfer
- **WHEN** knee has `t = 0.5`, `r = 0.25`, `o = 0`
- **THEN** inputs 0.25, 0.9 and −0.9 give 0.25, 0.6 and −0.6

### Requirement: Ping-pong and allpass delay modes
Delay kind 68 SHALL add these modes, each with a length in frames (`round(p·65536)`, at least 1) and a feedback gain g:

- `ping-pong` (`us-fx-delay-pingpong`): with `(wL, wR)` read from the line, the output is `yL = xL + g·wL` and
  `yR = xR + g·wR`. The line stores `(yR, yL)`, so the channels swap on every pass. There is no dry/wet control.
- `allpass` (`us-fx-delay-allpass`): with `(wL, wR)` read, `vL = xL + g·wR` and `vR = xR + g·wL`. The line stores
  `(vL, vR)`, and the output is `yL = wR − g·vL`, `yR = wL − g·vR` (a Schroeder allpass whose feedback crosses the
  channels).

#### Scenario: Ping-pong impulse
- **WHEN** an impulse enters the left channel of a ping-pong delay with length 100 and g = 0.5
- **THEN** the output has the impulse on the left at 0, 0.5 on the right at 100, and 0.25 on the left at 200

#### Scenario: Allpass energy
- **WHEN** white noise runs through an allpass mode with g = 0.7
- **THEN** the long-term output power equals the input power within 0.1 dB

### Requirement: Expanded effect mode choices
Adding modes SHALL NOT change what a stored mode value selects. The distortion, EQ and delay mode choices SHALL be
expanded so that every earlier mode `id` keeps its stored value at position `2·id`:
- distortion: 4 → 7 positions
- EQ: 3 → 5 positions
- delay: 3 → 5 positions

The new modes take odd positions: distortion atan 1, sine 3, knee 5; EQ svf-lfo 1; delay ping-pong 1, allpass 3. A
free position (EQ 3) SHALL fail at load with `BadChunk`. Mode ids, which code and converters use, stay as they were;
the new modes get the next ids. The old-layout migration SHALL decode against the mode counts it was written for.

#### Scenario: Existing chunks unchanged
- **WHEN** the existing effect goldens render in a build with the new modes
- **THEN** they are byte-identical, and every earlier mode's stored value selects the same mode

#### Scenario: Free position
- **WHEN** an EQ chunk stores position 3 of 5
- **THEN** the load fails with `BadChunk`
