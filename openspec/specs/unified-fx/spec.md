# unified-fx Specification

## Purpose
One set of effect devices that replaces the WaveSabre effects and the Tunefish effects. Each different legacy
algorithm is kept as a mode of one effect kind, and every effect works on any `synth-mix` track.

## Requirements

### Requirement: Effect kinds and modes
The effects SHALL be `synth-mix` device kinds 65 distortion (shaper, power), 66 crusher, 67 EQ (parametric,
3band), 68 delay (tempo, ms), 69 chorus, 70 flanger/phaser (phaser, ring), 71 reverb
(freeverb, compact, rooms), 72 compressor and 73 formant. A mode is named for what it does; the WaveSabre or Tunefish effect it comes from is only its origin. Each kind's first parameter SHALL select its
mode.

#### Scenario: Mode selection
- **WHEN** a reverb device is loaded with mode rooms and then with mode freeverb
- **THEN** their impulse responses show the rooms mode's delay-line arrivals and the freeverb mode's comb/allpass arrivals respectively

### Requirement: Legacy parameter layout per mode
After the mode parameter, each mode SHALL take its legacy effect's parameters in the legacy order, with the legacy
normalized ranges and value curves. Legacy effect parameters and their automation SHALL therefore convert by copying.

#### Scenario: Echo parameters
- **WHEN** a delay in tempo mode gets the 9 parameters of a WaveSabre Echo chunk at 120 BPM
- **THEN** its left and right delay times, feedback, cross-feedback, cuts and dry/wet equal the Echo's

### Requirement: Close to the legacy effect
Each mode SHALL match its legacy effect within the parity tolerances (1 dB integrated loudness, 3 dB per octave band
63 Hz–16 kHz, onsets matched) on that effect's legacy synthetic test songs, rendered by the legacy crate and by the
unified effect.

#### Scenario: Freeverb mode parity
- **WHEN** the WaveSabre Cathedral test song is rendered by `crates/wavesabre` and, converted, by the unified reverb
- **THEN** the parity report passes

### Requirement: Buffer roles and sidechain
Effects SHALL transform main L/R in place and SHALL NOT write the sidechain pair. The compressor SHALL take its key
from main or from the sidechain pair by a parameter.

#### Scenario: Sidechain key
- **WHEN** a compressor keyed by the sidechain receives a loud kick on the sidechain and a steady pad on main
- **THEN** the pad's level drops on every kick, and with the key on main it does not

### Requirement: Tempo and rate from the song
Effects SHALL take tempo and sample rate from the render context, not from constants, including modes whose legacy
effect assumed 44 100 Hz.

#### Scenario: ms delay at 48 kHz
- **WHEN** an ms-mode delay set to 250 ms runs at 48 000 Hz
- **THEN** its first echo arrives 12 000 frames after the input

#### Scenario: Formant at 48 kHz
- **WHEN** the formant vowel A runs at 48 000 Hz and at 44 100 Hz
- **THEN** its first two formant peaks are at the same frequencies in Hz (800 and 1150 Hz within 3 %)

### Requirement: Silent reverbs sleep
A reverb whose input and output have both been below −140 dBFS for 8192 frames SHALL stop processing and output silence
until its input returns, with its lines cleared (they hold nothing above that level), so an idle reverb costs nothing.

#### Scenario: Sleep and wake
- **WHEN** an impulse rings out and the input stays silent, then an impulse arrives again
- **THEN** the reverb is asleep (output exactly 0) and answers the second impulse like a fresh reverb within 1e-5 of its peak

### Requirement: Caller block size independence
An effect's output SHALL NOT depend on how a render is split into calls, including modes with block-rate control
(the chorus), which SHALL keep their control grid on absolute song frames.

#### Scenario: Odd call sizes
- **WHEN** a chorus renders a signal in calls of 7, 100 and 512 frames
- **THEN** all outputs are bit-identical

### Requirement: Effect features and reset
Each effect kind and each mode SHALL be a cargo feature. A song that uses a disabled kind or mode SHALL fail at load
naming it. `reset` SHALL return an effect to its freshly loaded state.

#### Scenario: Disabled mode
- **WHEN** a song uses reverb mode rooms and the rooms mode feature is off
- **THEN** loading fails with an unsupported-module error naming reverb mode rooms

#### Scenario: Reset
- **WHEN** an effect renders a signal, is reset, and renders the same signal again
- **THEN** both outputs are bit-identical

### Requirement: Caller-owned memory for effects
Effect state (delay lines, reverb buffers) SHALL come from the caller's pool, sized at load from the parameters and
the song's tempo and rate. An effect SHALL NOT allocate during rendering.

#### Scenario: Delay capacity
- **WHEN** a delay in tempo mode loads at 60 BPM with the longest coarse and fine settings
- **THEN** the reported pool size covers the delay line and the render never reads outside it

### Requirement: Comb reverb, modulated delay, look-ahead compressor and cut EQ modes
The effects SHALL have these modes: reverb `comb` (parallel damped combs, all-passes, low cut), delay `modulated`
(modulated stereo delay), compressor `lookahead` (peak or RMS detection, look-ahead, stereo link) and EQ `cut` (low
and high cut). Their parameters SHALL use 7-bit laws (value = step / 128) and SHALL have no
format-version parameter: the converter writes version-dependent behaviour (reverb low cut off), and the cut EQ's DC
filter is a setting. The other modes SHALL be unchanged.

#### Scenario: Existing modes unchanged
- **WHEN** the effect self-goldens recorded before this change are run
- **THEN** every hash still matches

#### Scenario: Reverb low cut by format version
- **WHEN** a song converted from a V2 format version below 4 uses the comb reverb
- **THEN** the converter sets the low cut to off, so the reverb runs without it, as V2 did before version 4

### Requirement: V2 modes match the reference
Each of the comb reverb, modulated delay, look-ahead compressor and cut EQ modes SHALL match its V2 original within
the parity tolerances (1 dB integrated loudness, 3 dB per octave band 63 Hz–16 kHz, onsets matched) on a synthetic
test song rendered by the reference player and, converted, by the unified effect.

#### Scenario: Delay parity
- **WHEN** a song with one plucked note into the V2 delay at full feedback is rendered both ways
- **THEN** the parity report passes

### Requirement: Comb flanger mode
The flanger kind SHALL gain a mode **comb** (`us-fx-flanger-comb`) with a delay K in frames at the song rate and an
amount g. With sweep rate 0 it outputs `y[n] = x[n] − g·x[n − K]`. The mode SHALL be stored at a free position of the
flanger's expanded mode choice, so the stored values of phaser and ring keep their meaning.

#### Scenario: Difference comb
- **WHEN** a sine at the frequency `rate/K` runs through comb with g = 1 and no sweep
- **THEN** the output is silent after the first K frames

### Requirement: Comb sweep
The comb mode SHALL have a sweep rate s and a restart parameter. With s above 0 it outputs
`(x[n − K] + x[n − K + s·t])/2`, where t counts frames since the last restart. A step of the restart parameter SHALL
set t to 0 at that frame.

#### Scenario: Sweep restarts
- **WHEN** the restart parameter steps during playback
- **THEN** the sweep's read offset returns to 0 at that frame

### Requirement: Wrap distortion mode
The distortion kind SHALL gain a mode **wrap** (`us-fx-distortion-wrap`) with a drive and an offset. It outputs
`v = x·drive + offset` wrapped into −1..1 (`((v + 1) mod 2) − 1`). The mode SHALL be stored at a free position of the
distortion's expanded mode choice, so the stored values of shaper, curve and power keep their meaning.

#### Scenario: Wrap-around
- **WHEN** a sample of value 0.9 passes through wrap with drive 1.25 and offset 0
- **THEN** the output is 1.125 − 2 = −0.875

#### Scenario: Old distortion values unchanged
- **WHEN** an existing song with a power-mode distortion loads in a build that has the wrap mode
- **THEN** the effect still runs power mode, and the song renders byte-identical to its golden

### Requirement: Effect chunk version
An effect chunk in the new layout SHALL carry a version marker. The player SHALL reject an effect chunk without it,
or with an unknown version, or truncated at any byte, with an error and no panic (the checked build). The host
migration SHALL convert old effect chunks.

#### Scenario: Old effect chunk rejected
- **WHEN** the player loads an effect chunk written before the format switch
- **THEN** loading fails with an unknown-version error

#### Scenario: Truncated effect chunk
- **WHEN** a new effect chunk is truncated at any byte
- **THEN** loading returns an error and does not panic

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
