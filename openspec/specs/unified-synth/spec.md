# unified-synth Specification

## Purpose
One instrument that holds the sound-making modules of both legacy engines (WaveSabre and the Tunefish-compatible
engine) in a fixed slot layout, so every sound of either engine can be built, mixed with the other's, and played
through the shared `synth-mix` song player.

## Requirements

### Requirement: no_std, no heap, MIT
The instrument SHALL build `no_std` for the i686 compo target and `wasm32-unknown-unknown`, with no heap and no
`core::fmt`, and SHALL be MIT licensed with the WaveSabre notice kept for WaveSabre-derived code.

#### Scenario: Compo and wasm builds
- **WHEN** the crate is checked for `i686-pc-windows-msvc` (core only) and `wasm32-unknown-unknown` with all features
- **THEN** both builds succeed and the linker map contains no `core::fmt` symbols

### Requirement: Device kind and chunk
The instrument SHALL be `synth-mix` device kind 64. Its chunk SHALL hold a format version, the parameter values that
differ from their defaults as index/value pairs in strictly increasing index order, and a sample section, with no
bytes after it. Parameters not in the chunk SHALL take their defaults.

#### Scenario: Sparse chunk
- **WHEN** a chunk sets only oscillator slot 1 type and level
- **THEN** the instrument loads, every other parameter has its default value, and a note renders sound

#### Scenario: Malformed chunk
- **WHEN** a chunk is truncated at any byte, has an unknown version, a parameter index out of range, repeated or unsorted indices, a non-finite or out-of-range value, or bytes after its last section
- **THEN** loading returns an error and does not panic

### Requirement: Sample section
The sample section SHALL hold up to 255 GSM sample chunks, each with its own size. Each entry SHALL parse as a
WaveSabre GSM sample chunk with no bytes after its compressed data and a size that fits the remaining chunk. Sampler
slots SHALL refer to entries by 1-based number.

#### Scenario: Bad entries
- **WHEN** an entry has size 0, a size past the end of the chunk, a bad GSM header, or bytes after its compressed data
- **THEN** loading returns an error and does not panic

#### Scenario: Index past the section
- **WHEN** a GSM sampler slot refers to sample 3 and the section holds 1 sample
- **THEN** loading returns an error

### Requirement: Structural parameters
Voice capacity, each oscillator slot's type, and each sampler slot's source and sample index SHALL be read from the
chunk only, because they shape the instrument's memory. `set_param` on them SHALL be ignored, so automation cannot
change what was sized at load.

#### Scenario: Automating a structural parameter
- **WHEN** a song automates oscillator 1 type from phase-mod to wavetable and the patch was loaded without a wavetable buffer
- **THEN** the render stays phase-mod and nothing reads outside the voice's memory

#### Scenario: Every shape value is automatable
- **WHEN** a song automates a wavetable slot's layer spread, the last of its shape values
- **THEN** the value is applied (only the sample index, held apart from the shape values, is raw and structural), and a chunk or song may hold 0.29 for it


### Requirement: Oscillator slots
A voice SHALL have 3 oscillator slots. Each slot SHALL be off or one of: phase-modulation operator (sine/square blend
with self-feedback), band-limited saw/pulse, spectral wavetable (with 1–10 unison layers, detune, spread, drive), or
sampler. Each slot SHALL have its own pitch (key-tracked ratio or fixed frequency, coarse, fine, octave), mix level
and pan.

#### Scenario: Pitch per slot
- **WHEN** note 69 plays with slot 1 at ratio 1 and slot 2 at ratio 2, both band-limited saws
- **THEN** the output has fundamentals at 440 Hz and 880 Hz

#### Scenario: Slot off
- **WHEN** every oscillator slot and the noise source are off
- **THEN** the instrument outputs silence

### Requirement: Start-phase spread
A voice parameter SHALL set how far the random start phase of its phase-modulation and saw/pulse slots reaches, as a
number of samples of the slot's own period from 0.01 to 10 000, the whole turn at its maximum, which is the default. A
saw/pulse slot SHALL take it at its first note only, as its phase runs on across notes.

#### Scenario: Legacy-like unison
- **WHEN** four voices of a saw at note 36 start together with a spread of 12.6 samples
- **THEN** their summed level is more than 3.6 times a single voice's, and with the default it is below 3 times

#### Scenario: Default spread
- **WHEN** a patch leaves the spread at its default
- **THEN** voices start from a phase drawn over the whole turn, as before the parameter existed

### Requirement: Phase modulation between slots
A slot SHALL be able to send its output into the phase of the next slot with an amount, independently of its mix
level, so a slot can act as a modulator only.

#### Scenario: Two-operator FM
- **WHEN** slot 1 (phase-mod operator, mix level 0) modulates slot 2 (phase-mod operator) at ratio 1 with a non-zero amount
- **THEN** the output spectrum has sidebands at multiples of the note frequency, and with amount 0 it is a pure sine

### Requirement: Open filters are passed through
A filter slot whose cutoff is at the top of its range (a low-pass or ladder) or at the bottom (a high-pass) with no
resonance SHALL be passed through unchanged and not run. When it stops being open its state SHALL start from the signal
that went past, so the change does not click. The bypass changes the sound a little (the legacy filters roll the top
octaves off), and the parity report SHALL list the songs judged against a wider tolerance for it.

#### Scenario: Open low-pass
- **WHEN** a state-variable low-pass is at its top cutoff with no resonance
- **THEN** the slot is not run and a 440 Hz note comes out within 1 % of the same note with the slot off

#### Scenario: Leaving the open position
- **WHEN** the cutoff of a passed-through low-pass moves down to 5 kHz mid-note
- **THEN** the first 200 frames stay within 0.03 of the output of a filter that ran the whole time


A voice SHALL have one noise source with a level and an optional band filter (centre and bandwidth). The noise SHALL
come from the voice's own deterministic generator. A width parameter (0..1, default 1) SHALL set how far left and
right differ: 1 gives each channel its own noise, 0 the same noise on both (WaveSabre's Slaughter), in between a mix
that keeps each channel's power.

#### Scenario: Width
- **WHEN** noise plays at width 0, 0.5 and 1
- **THEN** the L/R correlation is 1, between 0.5 and 0.9, and below 0.03, with equal power in each channel

#### Scenario: Band-limited noise
- **WHEN** noise plays with the band filter at 1 kHz and narrow bandwidth
- **THEN** the long-term spectrum peaks near 1 kHz and (at width 1) the L/R correlation is below 0.1

### Requirement: Filter slots
A voice SHALL run its summed oscillator and noise signal through 4 filter slots in series. Each slot SHALL be off or
one of: state-variable low-pass, high-pass, band-pass or notch; ladder low-pass; RBJ high-pass or band-pass; resonator
notch. A slot that is off SHALL pass the signal unchanged.

#### Scenario: Filter off is bypass
- **WHEN** all filter slots are off
- **THEN** the voice output equals the unfiltered oscillator sum

#### Scenario: Low-pass cutoff
- **WHEN** a state-variable low-pass at 1 kHz filters white noise
- **THEN** the level one octave above 1 kHz is at least 9 dB below the level one octave below it

### Requirement: Envelopes
A voice SHALL have 4 ADSR envelopes, and SHALL end when envelope 1 finishes. Envelope 1 SHALL scale the voice output
after the filters unless the patch sets it to act only through mod-matrix routes. Each envelope SHALL use one of two
curve modes: linear attack with quadratic decay/release (WaveSabre), or rate-based log steps with a slope (Tunefish).
Times SHALL be in seconds, independent of sample rate.

#### Scenario: Voice end
- **WHEN** a note is released and envelope 1's release of 0.1 s completes
- **THEN** the voice stops rendering and is free for a new note

#### Scenario: Envelope 1 through routes only
- **WHEN** envelope 1 is set to act only through routes and is routed (Scale) to oscillator slot 1 gain
- **THEN** the slot's signal is scaled before the filters, the voice output is not scaled again, and the voice still ends when envelope 1 finishes

#### Scenario: Rate independence
- **WHEN** the same patch renders at 44 100 Hz and 48 000 Hz
- **THEN** envelope 1 reaches sustain at the same time in seconds (within one control block)

### Requirement: LFOs
A voice SHALL have 2 LFOs with rate, depth and shape (sine, ramp up, ramp down, square, stepped random), each either
restarting at note-on or free-running across notes. A Scale slot SHALL see an LFO as `1 − d + d·shape` (unipolar,
d = depth²) and an Offset slot as `d·(2·shape − 1)` (bipolar).

#### Scenario: Key sync
- **WHEN** an LFO is set to restart at note-on and two notes start 0.3 s apart
- **THEN** both voices' LFO outputs at their own note-on equal the shape's start value

#### Scenario: Vibrato as an offset
- **WHEN** a sine LFO at depth 1 drives an Offset slot on pitch with amount 0.5
- **THEN** the pitch swings between −0.5 and +0.5 semitones around the note

### Requirement: Mod matrix
The voice SHALL have a 12-slot mod matrix: source (envelope 1–4, LFO 1–2, mod wheel, velocity, key), target, amount
and operation. A Scale slot SHALL multiply the target's normalized value (several Scale slots multiply together). An
Offset slot SHALL add amount × source in the target's physical unit after mapping (several add together).

#### Scenario: Scale composition
- **WHEN** two Scale slots aim at filter 1 cutoff with results 0.5 and 0.5
- **THEN** the cutoff mapping receives 0.25 × the base normalized value

#### Scenario: Pitch offset
- **WHEN** an Offset slot routes a fully open envelope to pitch with amount +12
- **THEN** the voice plays one octave above the note while the envelope is at 1

### Requirement: Modulation targets
Targets SHALL include voice pitch (all slots); each oscillator slot's pitch, gain, mix level, pan, phase-mod amount
and type-specific shape parameters; noise level and band; each filter slot's cutoff and resonance; envelope decay; LFO
depth; each mod slot's own amount; voice pan and level.

#### Scenario: Mod slot depth
- **WHEN** slot 1 routes LFO 1 to filter cutoff and slot 2 routes envelope 2 to slot 1's amount (Scale)
- **THEN** the LFO's effect on the cutoff follows envelope 2

### Requirement: Control rate
Envelopes, LFOs, the mod matrix and filter coefficients SHALL update once per 32-frame control block on the absolute
song frame grid, with linear interpolation of their outputs inside the block. For the first 256 frames after a note-on
and after a note-off a voice SHALL update on the absolute 8-frame grid instead, so that envelopes of a few
milliseconds keep their peak and their tail. Audio SHALL run per sample.

#### Scenario: Short envelopes
- **WHEN** a note has a 1 ms attack and a 3 ms decay
- **THEN** the energy of the rendered note is within 10 % of the energy of the same envelope stepped per frame (on the 32-frame grid alone a 3 ms hit lost about 1.7 dB)

#### Scenario: Caller and sub-block independence
- **WHEN** a song without automation renders with player sub-blocks of 100 and 37 frames and caller buffers of 64 and 44 100 frames
- **THEN** all outputs are bit-identical

### Requirement: Sample-accurate notes
A note event SHALL start or release its voice at its exact frame, not at a control-block boundary. An envelope starts
from 0 at the note's frame, so with the minimum attack the first non-zero sample is the frame after it.

#### Scenario: Onset frame
- **WHEN** a note-on arrives at offset 37 of a render call with the minimum attack
- **THEN** frames 0 to 37 are silent and frame 38 carries the first non-zero sample, and the same note at offset 38 starts one frame later

### Requirement: Voice allocation
The instrument SHALL have a voice capacity of 1–64 from its patch, a stealing policy (none or oldest), voice-level
unison (1–16 voices with detune and pan spread), poly or mono-legato mode with slide time, glide and a pitch-bend
range. Velocity 0 SHALL be a note-on, as in the shared device contract.

#### Scenario: Capacity with stealing
- **WHEN** a capacity-4, stealing-oldest instrument receives a 5th note-on
- **THEN** the oldest voice restarts with the new note

#### Scenario: Releasing voices that cannot be heard
- **WHEN** a voice is releasing and its output stays below −72 dBFS (mean magnitude 2.5e-4, in 256 frames) while envelope 1 has not finished
- **THEN** the voice is freed (an exponential release otherwise holds a voice for seconds after it is inaudible; the legacy Tunefish engine does the same with its block level), and a voice that is not releasing is never freed this way

#### Scenario: Mono legato slide
- **WHEN** in mono legato notes 60 and 64 are held and 64 is released
- **THEN** the sounding voice slides to 64 and back to 60 without retriggering envelope 1

### Requirement: Sampler sources
A sampler slot SHALL play one of three sources: a GSM 6.10 sample from the chunk's sample section, one wave of the
caller's GM.DLS bank, or one instrument of that bank. GSM and DLS-wave sources are chosen by a 1-based index (0 = none;
DLS wave: the legacy `(int)v − 1` truncation). A DLS-instrument source is chosen by `index = 1 + drums·16384 + bank·128
+ program`, so it finds the same instrument in any bank (Windows `gm.dls`, macOS `gs_instruments.dls`).

#### Scenario: Source values
- **WHEN** the source parameter reads 0.0, 0.5 and 1.0
- **THEN** they are GSM, DLS instrument and DLS wave (the first and last mean what they meant before the instrument source existed)

#### Scenario: No bank
- **WHEN** a sampler slot selects a GM.DLS wave and no bank is given
- **THEN** that slot is silent and the rest of the voice still sounds

### Requirement: DLS instrument regions
With a DLS-instrument source, each note-on SHALL pick the instrument region for its key and velocity and play that
region's wave with the region's (or wave's) root key, fine tune and loop, at the pitch its own sample rate implies. The
choice is fixed for the life of the voice. A key with no region, a missing instrument or a missing bank SHALL leave the
slot silent for that note. DLS articulation and the `wsmp` gain SHALL be ignored.

#### Scenario: Gain not applied
- **WHEN** a wave's `wsmp` holds −113 dB, as Apple's piano does, and its note plays
- **THEN** the note is not attenuated by it (the bank makes up for it in articulation the synth ignores; the patch's envelopes and gain shape the sound)

#### Scenario: Key split
- **WHEN** "Piano 1" (10 regions) plays notes 36 and 72
- **THEN** note 36 plays the wave of region 0–38 and note 72 plays the wave of region 68–74, each at its own root key

#### Scenario: Drum kit
- **WHEN** the "Standard" kit plays note 36 and note 38
- **THEN** each note plays its own drum wave at the region's root pitch, not one wave transposed

#### Scenario: Instrument missing from the bank
- **WHEN** the index names (bank 16, program 0) and the bank has no such instrument
- **THEN** the slot is silent and loading does not fail

### Requirement: Mod wheel and pitch bend parameters
The instrument SHALL expose the mod wheel (0..1) and pitch bend (0..1, centre 0.5) as automatable parameters, plus a
bend range of 0–24 semitones (default 2). The wheel value SHALL feed mod source 7; bend SHALL shift every voice's pitch
by `(bend − 0.5) · 2 · range` semitones, held or released. Both default to their rest values (wheel 0, bend centre), so
a song that never sets them sounds as before. The live input `mod_wheel` SHALL set the same parameter.

#### Scenario: Automated wheel
- **WHEN** a song automates the wheel from 0 to 1 and a mod slot routes wheel to filter 1 cutoff
- **THEN** the cutoff opens over that span, and seeking to the middle gives the same value as playing through

#### Scenario: Bend range
- **WHEN** bend range is 2 and bend is automated to 1.0 while note 60 is held
- **THEN** the voice sounds at note 62

### Requirement: Sampler playback
A sampler slot SHALL support start position, loop off/repeat/ping-pong, loop bounds from the sample or set by hand,
reverse, nearest or linear interpolation, and one-shot playback that ignores note-off. Its position SHALL advance by
one sample unit per output frame at unity pitch, as the legacy samplers do, so sample duration in seconds scales with
the song rate.

#### Scenario: Looped one-shot
- **WHEN** a one-shot voice plays a repeat-looped sample with a sustaining envelope
- **THEN** it keeps sounding after note-off until it is stolen or all notes are turned off (the converter never emits this)

#### Scenario: Unity pitch
- **WHEN** a GSM sample plays at its root note with coarse and fine at zero
- **THEN** the output equals the decoded sample (linear interpolation, no loop) up to the amplitude envelope

### Requirement: Determinism
Each instrument SHALL own a random source seeded at load from the song random source, and all its randomness (phases,
noise, slop, drift) SHALL come from it. The same song and pool SHALL render bit-identically on every run and target.

#### Scenario: Repeat render
- **WHEN** a song with noise and random start phases renders twice
- **THEN** both outputs are bit-identical

#### Scenario: Reset equals fresh load
- **WHEN** the player seeks to frame 0 and renders again
- **THEN** the output equals the first render

### Requirement: Module features
Each oscillator type, the noise source, each filter type and each envelope curve mode SHALL be a cargo feature. A
patch that uses a disabled module SHALL fail at load with an unsupported-module error naming it, not at render.

#### Scenario: Disabled oscillator type
- **WHEN** a song uses a wavetable slot and the crate is built without the wavetable feature
- **THEN** loading fails with an error naming the wavetable module

### Requirement: Caller-owned memory
All instrument state (voices, wavetables, decoded samples) SHALL come from the caller's pool. The required size SHALL
be computable from the chunk before loading with overflow-checked arithmetic, and per-voice buffers SHALL be sized
only for the module types the patch uses.

#### Scenario: Exact pool
- **WHEN** a song loads into a pool of exactly the reported size at every start alignment, and into one 1 byte smaller than the bytes that load consumed
- **THEN** the first loads and renders (loading consumes no more than the reported size), and the second fails with `OutOfMemory`

#### Scenario: Hostile sample sizes
- **WHEN** a chunk declares 255 samples of size 0xFFFFFFFF each (also on a 32-bit target)
- **THEN** sizing returns an error and nothing is allocated or wrapped around

### Requirement: Speed and size report
A speed build (the release profile at opt-level 3, `[profile.fast]`; the shipped player stays on the size-optimised
`release`) SHALL render each converted reference song at least as fast as its legacy engine's bar (Elysian ≥ 5×,
no_remorse ≥ 10× real time, one thread, dev machine); the report SHALL also give the figures on `release`. A report SHALL give raw and packed i686 sizes for an all-modules
build and for each reference song's feature set.

#### Scenario: Size report
- **WHEN** the size report runs
- **THEN** it prints raw and packed bytes per build next to the legacy engines' figures

### Requirement: Module code is removed with its feature
A build without a module's cargo feature SHALL NOT contain that module's code (in addition to rejecting patches that use
it at load): the compiled player for a song needs no more than the modules `synth-convert features` lists for it.

#### Scenario: Module code is absent
- **WHEN** a song that uses only the noise source and the state-variable filter is built with exactly its listed features
- **THEN** the build holds no phase-mod, saw/pulse, wavetable, sampler, ladder, RBJ or notch code (checked by symbol listing or size against the all-modules build)

#### Scenario: Explained map
- **WHEN** `synth-convert features --explain` runs on a song
- **THEN** each listed feature is followed by what requires it (device, parameter or automation lane)

### Requirement: Optional voice paths are features derived from the song
The voice paths a song may not use (LFOs, glide/slide, mono-legato, unison above one voice, one-shot, the mod matrix, automation lanes) SHALL each be a
cargo feature. `synth-convert features` SHALL list a path's feature exactly when a device or an automation lane of the
song can use it, and a build without it SHALL reject a patch that uses it at load with `UnsupportedModule`.

#### Scenario: A path the song does not use is left out
- **WHEN** a song whose instruments read no LFO, have no glide, mono, unison or one-shot is built with exactly its listed features
- **THEN** the build holds no LFO, pitch-move, note-log, unison or one-shot code and plays the song bit for bit as the full build

#### Scenario: A path reachable by automation is listed
- **WHEN** an automation lane can raise the glide time, switch mono mode on, or route a matrix slot to an LFO
- **THEN** `synth-convert features` lists the matching `us-voice-*` feature (and `us-matrix` for a routing lane) and `--explain` names the lane

### Requirement: A song can be compiled into the build as Rust
`synth-convert rust` SHALL write a unified song as Rust source that a compo build embeds instead of the blob. It SHALL
load the song with the player on the host first and refuse a song the player would refuse, SHALL keep the song's section
bytes unchanged, and the source SHALL fail to compile in a build that lacks a module the song needs. A build of a
generated song MAY leave out load-time validation (`us-trusted`); it SHALL render bit for bit what the blob renders.

#### Scenario: Same output as the blob
- **WHEN** a reference song is rendered from its blob with every module and from its generated Rust with exactly its features and `us-trusted`
- **THEN** the f32 output from frame 0 and the i16 output after seeks are identical, for sub-block sizes 100 and 77

#### Scenario: Missing module does not compile
- **WHEN** the generated Rust of a song that needs `us-voice-lfo` is built without that feature
- **THEN** compilation fails on the generated module assert
