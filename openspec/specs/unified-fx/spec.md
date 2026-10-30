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

### Requirement: V2 effect modes
The effects SHALL gain the modes reverb `v2`, delay `v2` (modulated stereo delay), compressor `v2` and EQ `cut` (V2's
post low cut and high cut). Each SHALL take V2's parameters in V2's order (value = V2 raw value / 128) with V2's laws.
Existing modes and their parameter layouts SHALL be unchanged.

#### Scenario: Existing modes unchanged
- **WHEN** the effect self-goldens recorded before this change are run
- **THEN** every hash still matches

#### Scenario: Reverb low cut by format version
- **WHEN** a song converted from a format version below 4 uses the V2 reverb
- **THEN** the reverb runs without its low-cut stage, as V2 did before version 4

### Requirement: V2 modes match the reference
Each V2 mode SHALL match V2 within the parity tolerances (1 dB integrated loudness, 3 dB per octave band 63 Hz–16 kHz,
onsets matched) on a synthetic test song rendered by the reference player and, converted, by the unified effect.

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
