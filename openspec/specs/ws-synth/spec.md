# ws-synth Specification

## Purpose
An MIT `no_std` Rust port of the WaveSabre device set (Logicoma), built on `synth-core` and played through `synth-mix`.
Existing WaveSabre songs and patches render with the same sound. Upstream source is the behavioural reference; the port
keeps the WaveSabre MIT notice.

## Requirements

### Requirement: Device set
`crates/wavesabre` SHALL provide the instruments Falcon, Slaughter, Specimen, Adultery and Thunder, and the effects
Scissor, Leveller, Crusher, Echo, Smasher, Chamber, Twister and Cathedral. Each SHALL be a cargo feature (`ws-<name>`,
default `ws-all`), implement `synth-core`'s `Device`, and register under its WaveSabre kind id.

#### Scenario: Minimal build
- **WHEN** the crate builds with only `ws-slaughter`
- **THEN** it compiles, and no code from the other devices is linked into the size probe

### Requirement: Parameter and chunk compatibility
Each device SHALL use upstream's parameter order and mapping from the normalized value to internal units. A device
chunk SHALL be read as `n` f32 parameters followed by an i32 chunk size, where `n = (size − 4) / 4`. Missing trailing
parameters keep their defaults and extra ones are ignored. Specimen and Thunder chunks SHALL additionally carry their
GSM sample (an 18-byte packed WAVEFORMATEX header, extra format bytes, compressed data and sizes) as upstream lays it out.

#### Scenario: Short chunk
- **WHEN** a Slaughter chunk holds fewer parameters than Slaughter defines
- **THEN** the given parameters are set, the rest keep their defaults, and nothing is read past the chunk

### Requirement: Sine table and phase precision
The port SHALL reproduce upstream's 512-entry linearly interpolated sine table used by `FastSin`/`FastCos` (oscillators, vibrato, filter coefficients) and keep double-precision phase accumulators where upstream uses them. `exp2` and `pow` SHALL use `libm` on every target; upstream's x87 paths are not reproduced.

#### Scenario: FastSin error
- **WHEN** FastSin is evaluated on 10 000 phases across [−4π, 4π]
- **THEN** it matches a reference model of the interpolated table exactly and `sin` within 1e-4

### Requirement: Random law
The WaveSabre random source SHALL compute `seed = seed ·wrapping 0x15A4E35` on a signed 32-bit integer and return `(seed % 255) / 255` with truncating remainder (range about [−1, 1]), starting from seed 1 at song load.

#### Scenario: Random law
- **WHEN** the song random source is drawn 5 times from seed 1
- **THEN** the values equal those computed by the reference formula with 32-bit wrapping arithmetic

### Requirement: Device algorithms
Instruments and effects SHALL use upstream's algorithms: Slaughter's integrated BLIT oscillator with leaky integrator 0.998, Falcon's two-operator phase modulation with feedback, the 2× oversampled state-variable filter, per-sample ms-based envelopes with quadratic decay/release, and each effect's upstream topology.

#### Scenario: Slaughter pitch
- **WHEN** Slaughter plays note 69 with one saw oscillator and the filter open
- **THEN** the fundamental of the output is 440 Hz within 0.5 Hz

### Requirement: Song-level random order
The WaveSabre random source SHALL be one per song: drawn first at load by device constructors in song device order (Slaughter: 3 phases for each of its 256 voices), then during rendering in track render order, matching upstream's single-threaded output. Seek SHALL restore its post-load state. Multithreaded rendering later SHALL require a spec change.

#### Scenario: Same as upstream single thread
- **WHEN** a song whose only randomness is Slaughter start phases renders
- **THEN** each voice's start phase equals upstream's single-threaded render for the same song

### Requirement: GSM samples
A `no_std` GSM 06.10 decoder in WAV49 layout (65-byte blocks → 320 samples) SHALL decode Specimen and Thunder samples
into pool memory at song load, as f32 = sample / 32768, truncated to the chunk's uncompressed length. A malformed sample
chunk (sizes past its end, a format other than mono GSM 6.10) SHALL fail the load with an error; every GSM bit pattern
decodes, as in libgsm. An empty chunk SHALL load as an empty sample that plays silence (upstream crashes).

#### Scenario: Decoder against libgsm
- **WHEN** a test vector block is decoded
- **THEN** the 320 samples equal libgsm's `GSM_OPT_WAV49` output exactly

### Requirement: GM.DLS sampler
Adultery SHALL read waves by sample index from a caller-supplied DLS bank parsed by `crates/dls` (the `gm-sound-bank`
capability), instead of upstream's fixed byte offset, reading the wave in place without copying. Without a bank, with an
unparsable bank, or for an index the bank lacks, it SHALL output silence. Any valid bank with ≥ 495
waves SHALL work (the Windows `gm.dls` gives upstream's sound; macOS `gs_instruments.dls` gives a similar sound).

#### Scenario: No bank
- **WHEN** a song with Adultery loads without a bank
- **THEN** it loads, renders, and the Adultery track is silent

### Requirement: Device parity
An opt-in tool SHALL build upstream WaveSabre natively on macOS from an uncommitted checkout and render each device from a chunk and a fixed note sequence (≤ 2 s) at the 100-frame step with one thread. Our render SHALL reach SNR ≥ 60 dB against it. No upstream code, songs or WAVs SHALL be committed.

#### Scenario: Parity tool absent
- **WHEN** the upstream checkout is not present
- **THEN** the parity test skips with a message and the normal test suite passes

### Requirement: Song parity
Full reference songs (upstream test refs, converted outside the repo) SHALL match upstream renders within 1 dB integrated loudness, 3 dB per octave band of the long-term spectrum from 63 Hz to 16 kHz, and onsets within one 100-frame step.

#### Scenario: Elysian
- **WHEN** Elysian renders with our player and with upstream at 1 thread
- **THEN** all three metrics are within tolerance

### Requirement: Performance and size
A release build SHALL render the Elysian reference song (217.8 s) at ≥ 5× real time single-threaded on the dev machine.
A size report SHALL give raw and packed i686 sizes for `ws-all` and for a Slaughter-only build.

#### Scenario: Render speed
- **WHEN** Elysian renders in release mode with one thread
- **THEN** it takes at most 43.6 s on the i9-9900K dev machine
