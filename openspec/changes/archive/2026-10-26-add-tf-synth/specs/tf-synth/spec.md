# Spec Delta

## Purpose

An MIT, `no_std` instrument engine that is sound-compatible with Tunefish 4. It plays existing Tunefish 4 patches with the same character inside a 64k intro, with no heap and deterministic output. The behavioural reference is `research/tunefish4-functional-spec.md`.

## ADDED Requirements

### Requirement: Fixed engine format
The engine SHALL run at 44 100 Hz in 512-sample blocks, in 32-bit float (64-bit only for the formant filter state), with 12-TET tuning at A4 = 440 Hz. It SHALL support up to 32 instruments of up to 16 voices each.

#### Scenario: Block rendering
- **WHEN** an instrument renders one block
- **THEN** it produces exactly 512 stereo frames and control-rate state (envelopes, LFOs, mod matrix) advances once

#### Scenario: Polyphony limit
- **WHEN** a patch sets `GenPoly` to p
- **THEN** at most `trunc(15p + 1)` voices of that instrument sound at once

### Requirement: no_std, no heap
The engine SHALL be `no_std`, SHALL NOT allocate, and SHALL NOT use `core::fmt`. All engine state SHALL live in caller-owned memory sized by compile-time constants. It SHALL build for the i686 Windows compo target and for `wasm32-unknown-unknown`.

#### Scenario: Compo and wasm builds
- **WHEN** the crate is checked for `i686-pc-windows-msvc` (`-Zbuild-std=core`) and for `wasm32-unknown-unknown`
- **THEN** both builds succeed

### Requirement: Patch compatibility
An instrument patch SHALL be 112 normalized values in [0, 1], in the Tunefish 4 parameter order and with its public names. Each value SHALL map to physical units by the formulas in the reference spec §5, so existing Tunefish 4 patches load without conversion.

#### Scenario: Parameter mapping
- **WHEN** `GenOct` = 0.5 and `GenUnis` = 0
- **THEN** the octave factor is 1 and one unison layer plays

#### Scenario: Wrong patch length
- **WHEN** a patch with fewer than 112 values is supplied
- **THEN** it is rejected and no instrument state changes

### Requirement: Spectral wavetable generator
Each voice SHALL build a single-cycle table by inverse FFT of a 512-bin magnitude spectrum. Partial h (1..`1+trunc(64·GenHarmn)`) is centred at bin `1 + s(2h−1)` with `s = 4·GenScale`, has a two-sided exponential profile of width `0.3 + GenBandw·h` cut at 5 widths, and gain `h^−(1+GenDamp)`. The phase stage reproduces the reference phase range. The table is played with unison, detune, spread, drive, octave, slop and glide.

#### Scenario: Harmonic series
- **WHEN** `GenScale` = 0.25 and one unison layer plays MIDI note n
- **THEN** the output spectrum has peaks at integer multiples of one fundamental, an octave above the note frequency

#### Scenario: Table refresh on change
- **WHEN** harmonics, bandwidth, damp, scale or band limit change
- **THEN** the table is rebuilt; otherwise it is reused

### Requirement: Noise, filters and envelopes
Each voice SHALL provide stereo noise with an optional band-pass, the four optional filters (low-pass ladder, high-pass, band-pass, notch with the reference bandwidth law and latency), two ADSR envelopes and two LFOs updated per block. Behaviour SHALL follow the reference spec §3.2–3.5.

#### Scenario: Filter off is bypass
- **WHEN** all filter enable parameters are off
- **THEN** the voice output equals the generator plus noise output

#### Scenario: Unrouted envelope gates
- **WHEN** no mod-matrix slot routes an ADSR to volume
- **THEN** the voice sounds at full level from note-on to note-off (organ gate)

### Requirement: Multiplicative mod matrix
Modulation SHALL be routed by 10 stored slots (source, amount, target), of which only the first 8 take effect. Each target value SHALL be multiplied by the product of its slot factors (parameter × amount × source). Slot-to-slot modulation SHALL have one block of delay.

#### Scenario: Slots 9 and 10 inert
- **WHEN** only slot 9 routes LFO1 to cutoff
- **THEN** the output equals the output with slot 9 empty

### Requirement: Voice allocation
A note-on SHALL take a free voice or, when all are busy, steal the oldest voice. Pitch bend SHALL be linear in Hz and apply to all voices of the instrument, including released ones. The mod wheel SHALL act as a mod-matrix source.

#### Scenario: Stealing
- **WHEN** polyphony is 2 and notes A, B, C start in order without note-offs
- **THEN** C takes A's voice and B keeps sounding

#### Scenario: Note-off of a stolen note
- **WHEN** a note-off arrives for a note whose voice was stolen
- **THEN** no other voice is released

### Requirement: Insert effects chain
Each instrument SHALL process up to 10 effect slots in order on its stereo bus. Slot type is `round(10p)`: 0 none, 1 distortion, 2 delay, 3 chorus, 4 flanger, 5 reverb, 6 formant, 7 EQ; 8–10 act as none. All slots share the instrument's effect parameters. Each kind SHALL be a separate cargo feature.

#### Scenario: Disabled kind
- **WHEN** a patch uses a kind whose feature is not compiled in
- **THEN** that slot acts as none and the other slots still process

#### Scenario: Reverb character
- **WHEN** a unit impulse goes through the reverb
- **THEN** the response comes from 8 parallel damped combs into 4 series allpasses per channel with Freeverb lengths, identical comb lengths on L and R, and the right allpasses longer by 23 samples

### Requirement: Instrument as a shared device
The Tunefish instrument SHALL implement `synth-core`'s `Device` as `synth-mix` kind 32 with its 112 parameters and an event lookahead of 511 frames. It SHALL fire each event at the start of the 512-frame block, on the absolute song grid (`ctx.frame`), that contains it.

#### Scenario: Same timing on a shared track
- **WHEN** a one-instrument `.tfm` song and an equivalent `synth-mix` song (one track with the kind-32 device and the same notes) render
- **THEN** both outputs are bit-identical before the player's output scaling

### Requirement: Standalone effects
Each effect kind SHALL also be a standalone `Device`: 33 distortion, 34 delay, 35 chorus, 36 flanger, 37 reverb, 38 formant, 39 EQ. Each has its own parameters (the effect's subset of the 112-param table, in table order), and its output SHALL NOT depend on the caller's block size.

#### Scenario: Standalone effect on another engine
- **WHEN** a kind-37 reverb is placed after a WaveSabre Slaughter on a track
- **THEN** its impulse response equals the reverb inside a Tunefish instrument with the same effect parameter values

#### Scenario: Caller block size
- **WHEN** a standalone chorus runs with caller blocks of 100 frames and of 512 frames
- **THEN** both outputs are bit-identical

### Requirement: No duplicated primitives
DSP that is the same algorithm as in `synth-core` (Freeverb comb/allpass, delay ring, DF1 biquad core, one-pole, maths) SHALL come from `synth-core` and SHALL NOT be reimplemented in `tfsynth`.

#### Scenario: Source check
- **WHEN** `crates/tfsynth/src` is searched for comb, allpass, ring-buffer or biquad state structs
- **THEN** none are defined there; the reverb, delay, chorus, flanger, HP/BP and EQ use the `synth-core` types

### Requirement: Effects auto-sleep
When an instrument's mean absolute output stays below 1e-5 for 2 seconds with no voice active, its effects SHALL stop processing and keep their state until a voice renders again.

#### Scenario: Tail cut after silence
- **WHEN** an instrument is silent for more than 2 s of rendered time
- **THEN** effect processing is skipped for its blocks until the next note-on

### Requirement: Deterministic output
All randomness (initial phases, noise, slop, generator drift) SHALL come from a fixed-seed generator owned by the engine. Rendering the same input twice SHALL produce bit-identical output.

#### Scenario: Repeat render
- **WHEN** the same patch and note sequence render twice from a fresh engine
- **THEN** both outputs are bit-identical

### Requirement: Quirk policy
Sonic quirks of the reference SHALL be reproduced; defects that cause crashes or undefined behaviour SHALL be fixed without audible change. Reproduced: phase-stage range, 8 live mod slots, block-rate control, flanger counter flip, auto-sleep, log₁₀2 HP/BP bandwidth, linear-Hz bend. Fixed: right-channel phase overrun, unwrapped chorus phase, unbounded flanger counter, uninitialized voice state.

#### Scenario: First sample in range
- **WHEN** a voice starts with maximum right-channel phase offset
- **THEN** the first table read stays inside the table and the output is finite

#### Scenario: Long run stability
- **WHEN** chorus and flanger run for 10 hours of simulated time
- **THEN** all outputs stay finite and the effect sweep keeps its rate

### Requirement: Real-time headroom
Rendering SHALL be fast enough to prerender a song well before playback: at least 10× real time for a full 16-instrument song on the dev machine (release build).

#### Scenario: Render speed
- **WHEN** the reference test song is rendered in a release build on the dev machine
- **THEN** wall time is at most one tenth of the song duration
