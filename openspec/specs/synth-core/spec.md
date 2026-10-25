# synth-core Specification

## Purpose
The shared synth layer used by every engine (WaveSabre port, Tunefish-compatible engine, later ones). It holds the DSP
primitives that are the same algorithm in several engines, and the framework every instrument and effect plugs into, so
any device can sit on any track. Algorithm survey: `research/softsynth-research.md`, "Tunefish 4 + WaveSabre".

## Requirements

### Requirement: no_std, no heap, MIT
`synth-core` SHALL be `no_std`, f32, SHALL NOT allocate and SHALL NOT use `core::fmt`. It SHALL depend only on `libm`. It
SHALL build for `i686-pc-windows-msvc` (core only) and `wasm32-unknown-unknown`.

#### Scenario: Compo and wasm builds
- **WHEN** the crate is checked for `i686-pc-windows-msvc` with `-Zbuild-std=core` and for `wasm32-unknown-unknown`
- **THEN** both builds succeed and no `core::fmt` symbol is linked into the compo probe

### Requirement: Reverb and delay primitives
`synth-core` SHALL provide the Freeverb comb (`out = buf[i]; z = out·(1−d) + z·d; buf[i] = in + z·fb`), the Freeverb allpass (`out = buf[i] − in; buf[i] = in + buf[i]·fb`) and a circular delay line with integer read, fractional (linear) read, and resize that either clears the buffer (WaveSabre) or keeps its contents with the position clamped (Tunefish). Lengths and coefficients are set by the engine.

#### Scenario: Comb impulse response
- **WHEN** a comb of length L with feedback 0.5 and damping 0 receives a unit impulse
- **THEN** the output has peaks of 1, 0.5, 0.25 at samples L, 2L, 3L and zero elsewhere

#### Scenario: Fractional delay read
- **WHEN** a ramp is written and read back at a delay of 10.5 samples
- **THEN** each output is the mean of the samples at delays 10 and 11

### Requirement: Filter primitives
`synth-core` SHALL provide a Direct Form I biquad core (5 coefficients, 4 history values) with RBJ coefficient designers as separate functions, so engines can add their own designers, and a one-pole filter.

#### Scenario: Biquad matches its designer
- **WHEN** a lowpass biquad is designed at 1 kHz, Q 0.707, and fed sines at 100 Hz, 1 kHz and 10 kHz
- **THEN** the measured gains are within 0.1 dB of the analytic response (≈0, −3, ≈−40 dB)

### Requirement: Maths helpers
`synth-core` SHALL provide note→Hz (`440·2^((n−69)/12)`), dB↔gain, equal-power pan (`√(1−p)`, `√p`), clamp, mix and exp2, all computed with `libm` so results are the same on every target.

#### Scenario: Note to frequency
- **WHEN** notes 69 and 81 are converted
- **THEN** the results are 440 Hz and 880 Hz

### Requirement: Device interface
Every instrument and effect SHALL implement one `Device` trait: `note_on(note, velocity, offset)` and `note_off(note, offset)` with a sample offset from the next call's first frame (effects ignore notes), `set_param(index, value)` (out-of-range index ignored), a constant `param_count()`, `all_notes_off()`, `reset()`, and `run(io, n, ctx)` for any `n` ≥ 1.

#### Scenario: Any block length
- **WHEN** the same device and events render once as one 4096-frame block and once as blocks of 1, 100 and 3995 frames with the same event frames
- **THEN** both outputs and the frames at which events fire are bit-identical, for devices whose upstream reference is also block-size independent

#### Scenario: Effect on an instrument's output
- **WHEN** an effect device runs on a buffer produced by any instrument device
- **THEN** it processes it without knowing which engine produced it

### Requirement: Buffer roles
`io` SHALL hold 4 channel buffers in the order main L, main R, sidechain L, sidechain R, processed in place. An instrument SHALL overwrite main L/R with its output (upstream WaveSabre semantics). An effect SHALL transform main L/R and MAY read the sidechain pair as a key. No device SHALL write the sidechain pair or frames past `n`.

#### Scenario: Instrument replaces main input
- **WHEN** a track receives a signal into main and its first device is an instrument
- **THEN** the instrument's output replaces the received signal, and the sidechain pair is unchanged

### Requirement: Reset
`reset()` SHALL return a device to its freshly loaded state: parameters and values derived from them kept; voices, queued events, delay contents, phases, counters and engine-owned random state cleared. On seek the player SHALL reset every device and re-seed the song random source before re-rendering from frame 0.

#### Scenario: Reset equals fresh load
- **WHEN** a device renders a note for 1 s, is reset, and renders the same note again with the song random source re-seeded
- **THEN** the second render is bit-identical to the first

### Requirement: Event lookahead
A device MAY declare a constant `event_lookahead()` L (default 0). In a call covering song frames [t, t+n), the mixer SHALL deliver, exactly once, every not-yet-delivered event with frame < t + n + L, with its offset counted from t (it may exceed n). A device rendering internal blocks of B frames needs L = B − 1, independent of the mixer's sub-block size.

#### Scenario: Event lookahead
- **WHEN** a device declares L = 511, the mixer uses 100-frame calls, and the song has a note at frame 1023
- **THEN** the device receives it exactly once, in the call starting at frame 500 (1023 < 500 + 100 + 511), with offset 523

### Requirement: No global device state
A device SHALL NOT read or write global mutable state. Sample rate, tempo (BPM), the song frame of the call's first frame, and the song's random source SHALL be passed in the render context `ctx`.

#### Scenario: Two songs side by side
- **WHEN** two players with different tempos render in the same process, interleaved call by call
- **THEN** each output equals that song rendered alone

### Requirement: Voice pool
`synth-core` SHALL provide a fixed-capacity voice pool generic over the voice type, with a stealing policy (`None`: drop
the note when no voice is free; `Oldest`: restart the voice with the most blocks since note-on, lowest index on ties), a
mode (`Poly`, `MonoLegato` with slide to the latest held note), and unison spread (n voices with detune and pan spread
evenly across the voices).

#### Scenario: No stealing
- **WHEN** a pool with policy `None` and 4 voices receives a 5th note-on while 4 voices are sounding
- **THEN** the 5th note is ignored and the 4 voices continue unchanged

#### Scenario: Oldest stealing
- **WHEN** a pool with policy `Oldest` and 4 voices receives a 5th note-on
- **THEN** the voice started first restarts with the new note

#### Scenario: Mono legato
- **WHEN** in `MonoLegato` notes 60 and then 64 are held and 64 is released
- **THEN** the sounding voice slides to 64 and back to 60 without retriggering

### Requirement: Caller-owned memory
Large state (delay lines, reverb buffers, sample data) SHALL be carved from a caller-provided `&mut [u8]` bump `Pool`,
aligned per type. Running out SHALL return an `OutOfMemory` error, never panic. Each song SHALL report its required pool
size before allocation.

#### Scenario: Pool exhaustion
- **WHEN** a song needs more pool bytes than provided
- **THEN** loading returns `OutOfMemory` and no device is partially built

### Requirement: Determinism
Rendering SHALL be deterministic: the same song, pool size and block sequence produce bit-identical output on every run
and target. Randomness SHALL come from an explicit `Rng` passed in the render context, with the engine choosing the law
and seed. A denormal guard SHALL flush subnormals on x86 (MXCSR FTZ/DAZ) and AArch64 (FPCR.FZ) for the duration of a
render call and restore the previous mode afterwards.

#### Scenario: Repeatable render
- **WHEN** a song renders twice
- **THEN** both outputs are bit-identical

#### Scenario: Float mode restored
- **WHEN** a render call returns
- **THEN** the caller's floating-point control register equals its value before the call
