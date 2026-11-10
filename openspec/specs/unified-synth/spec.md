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
The instrument SHALL be `synth-mix` device kind 64. Its chunk SHALL hold a format version, the non-default parameter
values as index/value pairs in strictly increasing index order, a sample section and (version 4 only) a speech text
section, with nothing after it; absent parameters take their defaults. Without speech text the chunk SHALL be version
3. The player SHALL reject versions 1 and 2 (the earlier layout) with an error; the host migration converts them.

#### Scenario: Sparse chunk
- **WHEN** a chunk sets only oscillator slot 1 type and level
- **THEN** the instrument loads, every other parameter has its default value, and a note renders sound

#### Scenario: Malformed chunk
- **WHEN** a chunk is truncated at any byte, has an unknown version, a parameter index out of range, repeated or unsorted indices, a non-finite or out-of-range value, or bytes after its last section
- **THEN** loading returns an error and does not panic (the checked build; a baked build trusts the song it was
  generated with and is not required to check)

#### Scenario: Speech text section
- **WHEN** a patch with speech text is encoded and loaded again
- **THEN** the chunk has format version 4, the text round-trips byte for byte, and the same patch without text encodes as version 3

#### Scenario: Old layout rejected by the player
- **WHEN** the player loads a format version 1 chunk
- **THEN** loading returns an unknown-version error and does not panic

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
A voice SHALL have 3 oscillator slots. Each slot SHALL be off or one of these types:
- phase-modulation operator (sine/square blend with self-feedback)
- band-limited saw/pulse
- spectral wavetable (1–10 unison layers, detune, spread, drive)
- sampler
- a naive generator (see Naive oscillator generators)

Each slot SHALL have its own pitch (key-tracked ratio or fixed frequency, coarse, fine, octave), mix level, pan and
combine operation.

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
one of the topologies: state-variable, 4-pole, RBJ biquad (high-pass or band-pass), resonator notch. A slot that is off
SHALL pass the signal unchanged.

#### Scenario: Filter off is bypass
- **WHEN** all filter slots are off
- **THEN** the voice output equals the unfiltered oscillator sum

#### Scenario: Low-pass cutoff
- **WHEN** a state-variable low-pass at 1 kHz filters white noise
- **THEN** the level one octave above 1 kHz is at least 9 dB below the level one octave below it

### Requirement: Envelopes
A voice SHALL have 4 ADSR envelopes, and SHALL end when envelope 1 finishes. Envelope 1 SHALL scale the voice output
after the filters unless the patch routes it only through the mod matrix. Each envelope's curves, time law, sustain
slope and amplify SHALL be its settings (see Envelope settings). Times SHALL be in seconds at any sample rate.

#### Scenario: Voice end
- **WHEN** a note is released and envelope 1's release of 0.1 s completes
- **THEN** the voice stops rendering and is free for a new note

#### Scenario: Envelope 1 through routes only
- **WHEN** envelope 1 is set to act only through routes and is routed (Scale) to oscillator slot 1 gain
- **THEN** the slot's signal is scaled before the filters, the voice output is not scaled again, and the voice still ends when envelope 1 finishes

#### Scenario: Rate independence
- **WHEN** the same patch renders at 44 100 Hz and 48 000 Hz
- **THEN** envelope 1 reaches sustain at the same time in seconds (within one control block)

#### Scenario: V2 sustain slope
- **WHEN** an envelope has sustain slope below the centre and a note is held
- **THEN** the level falls during the sustain stage, and at the centre value it stays constant

### Requirement: LFOs
A voice SHALL have 2 LFOs with rate, depth, shape and the settings of LFO settings, restarting at note-on or
free-running. A Scale slot SHALL see an LFO as `1 − d + d·shape` (unipolar, d = depth²), an Offset slot as
`d·(2·shape − 1)` (bipolar), an Add slot as the value for its polarity (0..1, −1..0, −1..1) × amplify.

#### Scenario: Key sync
- **WHEN** an LFO is set to restart at note-on and two notes start 0.3 s apart
- **THEN** both voices' LFO outputs at their own note-on equal the shape's start value

#### Scenario: Vibrato as an offset
- **WHEN** a sine LFO at depth 1 drives an Offset slot on pitch with amount 0.5
- **THEN** the pitch swings between −0.5 and +0.5 semitones around the note

#### Scenario: Envelope mode
- **WHEN** a saw LFO at 2 Hz set to one-shot is key-synced and a note is held for 2 s
- **THEN** the LFO completes one ramp in 0.5 s and then holds its end value

### Requirement: Mod matrix
The voice SHALL have a 16-slot mod matrix: source (envelope 1–4, LFO 1–2, mod wheel, velocity, key, breath, CC 3–6,
volume), target, amount and operation. Scale slots SHALL multiply the target's normalized value together; Offset slots
SHALL add amount × source in the target's physical unit after mapping. Slots 1–10 SHALL be one-byte chunk parameters
and slots 11–16 two-byte ones, evaluated only with the Add module; every slot SHALL default to no source.

#### Scenario: Scale composition
- **WHEN** two Scale slots aim at filter 1 cutoff with results 0.5 and 0.5
- **THEN** the cutoff mapping receives 0.25 × the base normalized value

#### Scenario: Pitch offset
- **WHEN** an Offset slot routes a fully open envelope to pitch with amount +12
- **THEN** the voice plays one octave above the note while the envelope is at 1

#### Scenario: A slot past 10 needs the Add module
- **WHEN** a patch routes mod slot 11 and the build lacks the matrix Add module
- **THEN** loading fails with an unsupported-module error naming it

### Requirement: Mod matrix Add slots
An Add slot SHALL add amount × source to the normalized value of its parameter target before the law, clamping to
0..1 after each slot in slot order (V2's rule: amount −1..1, source 0..1).

#### Scenario: V2 add with clamp
- **WHEN** a V2 filter's cutoff value is 0.75 and two Add slots route velocity 1.0 with amounts +0.5 and −0.125
- **THEN** the cutoff law receives 0.875 (0.75 + 0.5 clamps to 1.0, then 1.0 − 0.125), as V2's per-entry clamp gives

#### Scenario: Controller source
- **WHEN** an Add slot routes the volume controller to the channel volume and the song sets controller 7 to 0
- **THEN** the channel volume law receives its base value minus the amount

### Requirement: Modulation targets
Targets SHALL include voice pitch; each oscillator slot's pitch, gain, mix, pan, phase-mod amount and type-specific
shape; noise level and band; each filter slot's cutoff and resonance; envelope decay; LFO depth; each mod slot's own
amount; voice pan and level. Chunk v1 target ids SHALL keep their meaning. Every appended V2 parameter V2 can
modulate, channel stage included, SHALL get a new appended id. Channel stage targets SHALL use the instrument's newest
sounding voice, as in V2.

#### Scenario: Mod slot depth
- **WHEN** slot 1 routes LFO 1 to filter cutoff and slot 2 routes envelope 2 to slot 1's amount (Scale)
- **THEN** the LFO's effect on the cutoff follows envelope 2

#### Scenario: Channel target from the newest voice
- **WHEN** an Add slot routes velocity to the reverb send, and a note at velocity 127 is followed by one at velocity 32
- **THEN** after the second note-on the reverb send follows velocity 32

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

### Requirement: Escaped parameter indices
The chunk SHALL store an index below 240 as one byte and a higher index as a bank prefix byte (0xF0 + bank) followed
by one byte, giving indices up to 4335. It SHALL store a pair count below 255 as one byte and a higher count as 0xFF
followed by a 16-bit count. Each index and count SHALL have exactly one valid encoding (the shortest), so the encoding
stays canonical. Chunks that use neither escape SHALL keep their meaning and their format version.

#### Scenario: High index round trip
- **WHEN** a patch sets a parameter with index 250 and one with index 190
- **THEN** the encoded chunk holds 190 as one byte and 250 as two bytes (0xF0, 10), and decoding returns both values

#### Scenario: Non-canonical escape
- **WHEN** a chunk encodes a count below 255 as an escaped count
- **THEN** loading returns an error and does not panic (a two-byte index always decodes to 240 or more, so an index has only one form)

#### Scenario: Escape at the end
- **WHEN** a chunk ends right after a bank prefix byte or an escaped count byte
- **THEN** loading returns an error and does not panic

### Requirement: Naive oscillator generators
An oscillator slot SHALL offer naive (not band-limited) generators: saw↔triangle (colour morphs saw to triangle),
pulse (colour = width), sine, coloured noise, FM (phase modulated by the running sum of the earlier slots) and aux
input A/B. Transpose, detune, colour and volume SHALL follow the slot's pitch law (semitone/Hz laws, or 7-bit steps).

#### Scenario: Pulse width
- **WHEN** a naive pulse generator plays note 57 with colour at the centre
- **THEN** the output is a square wave at 220 Hz with odd harmonics only (even harmonics at least 30 dB lower)

#### Scenario: Aux input mode
- **WHEN** a slot in aux input A mode receives a 440 Hz sine on the device's aux input pair 1
- **THEN** the slot's output is that sine × its volume

### Requirement: Ring combine
Ring SHALL be the multiply value of the slot combine operation, available to every slot type; it multiplies the slot
into the running sum of the earlier slots instead of adding it.

#### Scenario: Ring mode
- **WHEN** slot 1 is a naive sine and slot 2 a naive sine one octave higher set to multiply
- **THEN** the output holds the sum and difference frequencies and not the two input tones

### Requirement: Filter routing
A voice routing parameter SHALL run filter slots 1 and 2 serially (the default) or in parallel, mixed by a balance
parameter, whatever their topologies. Slots 3 and 4 SHALL follow in series.

#### Scenario: Parallel balance
- **WHEN** slot 1 is a state-variable low-pass at 200 Hz and slot 2 a state-variable high-pass at 5 kHz in parallel at full balance towards slot 2
- **THEN** the output equals slot 2's output alone

#### Scenario: All-pass
- **WHEN** white noise runs through a state-variable all-pass
- **THEN** the octave-band levels are within 0.5 dB of the input

### Requirement: Voice distortion stage
A voice SHALL have one drive stage after the filters with the modes off, overdrive, clip, bit crush, decimate, and the
filter modes low-pass, band-pass, high-pass, notch, all-pass and 4-pole low-pass, with input gain and two mode
parameters on 7-bit laws. Off SHALL be the default, and an off stage SHALL not change the signal.

#### Scenario: Overdrive
- **WHEN** a full-scale sine runs through overdrive at high input gain
- **THEN** the output peak stays at or below 1 and odd harmonics appear

### Requirement: Channel stage
An instrument SHALL have one channel strip after the voice sum: channel volume, bass boost, channel drive (the voice
drive modes), chorus/flanger, a look-ahead channel compressor, and the order of drive and chorus. It SHALL have four
send levels (reverb, delay, aux A, aux B) whose signals leave the device as extra output pairs. The defaults SHALL make
the strip transparent, with every send at zero.

#### Scenario: Transparent default
- **WHEN** a patch leaves the channel strip at its defaults
- **THEN** the main output is bit-identical to the output without the strip, and the send outputs are silent

#### Scenario: Reverb send
- **WHEN** the reverb send is at full level
- **THEN** the reverb output pair carries the channel signal after the strip

### Requirement: Speech stage
An instrument SHALL have an optional formant speech stage on the voice sum, before the channel strip. Its chunk SHALL
hold the phoneme text. Each note-on SHALL advance the text by one syllable, and controllers 1–7 SHALL steer it (text
select and rate, pitch). Off SHALL be the default.

#### Scenario: Speech text
- **WHEN** an instrument with the speech stage on and the text of fr-08's speech channel plays that channel's notes
- **THEN** its output passes the parity metrics against the reference render of that channel

### Requirement: Shared-algorithm module features
Each optional module SHALL be a `us-*` cargo feature named by function:
- the naive generators
- the filter forms and laws
- parallel routing
- the voice drive stage
- the envelope and LFO laws and curves
- the channel strip
- speech
- aux input and output pairs
- each quirk flag

A patch that needs a disabled one SHALL fail at load with an unsupported-module error naming it, and the song feature
list SHALL name it.

#### Scenario: Speech disabled
- **WHEN** a patch with the speech stage on loads in a build without the speech feature
- **THEN** loading fails with an unsupported-module error naming the speech module

### Requirement: Read-only voice probe
A host-only cargo feature SHALL let a caller read the instrument's voice state at the end of a render call: per active
voice its note, amplitude, release state, envelope levels and stages, LFO phases, sampler play positions and filter
cutoffs, plus per-filter input/output taps summed over voices when enabled at run time. The probe SHALL only read.
Builds without the feature SHALL be unchanged in code and output.

#### Scenario: Goldens with the probe
- **WHEN** the instrument goldens run with the probe feature on and taps enabled
- **THEN** every golden hash matches the build without the feature

#### Scenario: Player size
- **WHEN** the compo player is built
- **THEN** it does not enable the probe feature and its size is unchanged

#### Scenario: Envelope state
- **WHEN** a note is held past attack and decay with sustain 0.5 and the probe is read
- **THEN** the voice reports envelope 1 in the sustain stage at level 0.5

### Requirement: Clocked oscillator slot type
An oscillator slot SHALL offer the type **clocked** behind the feature `us-osc-clocked`. It produces the
mini-cauldron waves (saw, square, spike, pulse, noise, fold) on a 16-bit phase, with naive (not band-limited) output.
Its waveform values SHALL equal the original renderer's for the same phase, pitch increment and frame count, scaled
by 1/32768.

#### Scenario: Saw matches the original
- **WHEN** a clocked slot with wave saw, voices 0 plays a note whose increment equals the original pitch word 799 at the original rate
- **THEN** its output over one second equals the original oscillator's 16-bit saw scaled by 1/32768, sample for sample

### Requirement: Clocked voices and version
The clocked slot's **voices** field (0–10) SHALL add the original second saw at a ×1.02 increment, or the spike's
detuned copy. The **version** field SHALL select v1 or v2 behaviour. Version 2 uses the spike without sign handling,
the 16-bit truncated detune offset, and a clamp of the oscillator output to full scale. Version 2 in a glide SHALL
hold a word above 32767 as its signed value for the second saw and the spike, as the original's glide register.

#### Scenario: Glide register
- **WHEN** a version-2-in-a-glide slot with voices 1 plays word 51199 for one frame
- **THEN** its phase steps by 51199 and its second saw by −14624 (not 52223) of 65536

#### Scenario: Spike wave
- **WHEN** a version-1 clocked slot plays wave spike with voices 3
- **THEN** each half cycle is a narrow peak of 32767·u⁷ shape, plus a second copy detuned by +3/128 that drifts with the frames since the phrase's first note

### Requirement: Clocked note-on phase
A non-legato note-on SHALL set the clocked phase to −32766/32768 of a cycle. It SHALL keep the second voice's phase,
unless the slot's phase byte (b ≥ 1) sets it to `(b − 1)/255` of a cycle. A legato note SHALL keep both.

#### Scenario: Phase byte
- **WHEN** a non-legato note-on finds the phase byte 129
- **THEN** the second voice starts at 128/255 of a cycle

#### Scenario: Legato keeps the phase
- **WHEN** a legato note follows a sounding note on a mono clocked instrument
- **THEN** the phase continues without a jump

### Requirement: Clocked filter
The clocked slot SHALL run the original resonant low-pass on its output when its filter base or amount is above 0,
with base, envelope amount, decay per phrase frame and damping in the original's phrase-frame units. At one phrase
frame per output frame its output SHALL equal the original's within 1e-4 of the peak.

#### Scenario: Matches the original filter
- **WHEN** a clocked slot with the filter on plays the reference test cases at one phrase frame per output frame
- **THEN** its output is within 1e-4 of the peak of the original filter's output, including a retrigger

#### Scenario: Off by default
- **WHEN** the filter base and amount are 0
- **THEN** the slot's output is unfiltered and existing goldens are unchanged

### Requirement: Clocked per-note compounding
The clocked slot SHALL compound its filter base and amount per note: times its compounding factor once per count
in the note's retrigger byte (`2k + accent`), each product rounded to f32, then times the accent (×4 version 1, ×2
version 2).

#### Scenario: Fifth retrigger with accent
- **WHEN** a version-1 note has the retrigger byte 11
- **THEN** its base and amount are the stored ones times the factor five times over, times 4

### Requirement: Clocked phrase position
A non-legato clocked note-on SHALL set the phrase frame counter to `(velocity − 1)` eighths of a 3530-frame step.

#### Scenario: Step start
- **WHEN** a note-on has velocity 41
- **THEN** the phrase frame counter is 5 × 3530

### Requirement: Clocked filter note-on
A non-legato note-on SHALL restart the clocked filter as the original's retrigger does: output memory 0, envelope
part = the amount, the rest of the state running on. A legato note SHALL keep it running. Since the restart zeroes
the output, a restart that cuts a sounding note SHALL carry the step from its last output frame to the new note's
first into the new note and fade it out linearly over 1 ms; a note from silence (below −80 dB) carries nothing.

#### Scenario: A sounding note cut by a restart
- **WHEN** a one-voice clocked instrument restarts while its filtered note sounds
- **THEN** the output does not drop to 0 in one frame: the step at the restart is no larger than twice the wave's own largest step

#### Scenario: Rising cutoff
- **WHEN** the decay is above 1 and a legato note follows
- **THEN** the envelope part of the cutoff keeps growing across the note

### Requirement: Clocked playback rate

A voice with a clocked slot SHALL treat its voice-wide transpose and bend as a playback rate: the slot's phrase
clock and the voice's envelopes SHALL advance `2^((transpose + bend)/12)` times as fast, leaving the pitch word
unchanged, as an XM pitch slide resamples the precalc sample.

#### Scenario: An octave up plays the phrase twice as fast
- **WHEN** a clocked voice with a rising filter is transposed (or bent) +12 semitones
- **THEN** after 1100 output frames its filter envelope has grown 2200 phrase frames, and a 200 ms release after a
  note-off ends in about half the time

### Requirement: Clocked word byte
A clocked slot SHALL read its fixed-frequency field as a word byte: b ≥ 1 makes its pitch word the frequency's word
rounded to an integer plus `b − 128`; 0 SHALL leave the frequency's word unchanged.

#### Scenario: Correction
- **WHEN** the frequency asks for word 51199.3 and the word byte is 125
- **THEN** the slot steps by 51196 of 65536 per phrase frame

### Requirement: Clocked phrase clock
The clocked slot (oscillator and filter) SHALL run at its phrase clock and be resampled to the output rate, so its
naive waveforms alias as the original's do at its own rate.

#### Scenario: Half rate
- **WHEN** the slot runs at half a phrase frame per output frame
- **THEN** every second output sample is a phrase-rate sample and the others lie between their neighbours

### Requirement: Clocked voice movement
In a voice with a clocked slot, a mono slide SHALL be linear in frequency, and a timed envelope attack of 0 SHALL
be instant.

#### Scenario: Octave glide
- **WHEN** a clocked voice slides up an octave
- **THEN** halfway through the slide its frequency is 1.5 times the start, not √2 times

### Requirement: Clocked type storage
The clocked type SHALL be stored at a free position of the expanded oscillator-type choice, so stored values of
existing types keep their meaning. A patch using it in a build without the feature SHALL fail at load naming the
module.

#### Scenario: Existing songs unchanged
- **WHEN** the existing golden songs are rendered in a build with `us-osc-clocked`
- **THEN** they are byte-identical to their goldens

#### Scenario: Missing module
- **WHEN** a patch with a clocked slot loads in a build without `us-osc-clocked`
- **THEN** the load fails with `UnsupportedModule` naming the clocked oscillator

### Requirement: Quiet-track answers
With feature `us-quiet` the unified devices SHALL allow the quiet-track skip. The instrument SHALL answer `quiet_after()` 0 only without sounding voices, queued events, channel stage and speech; the shaper, EQ 3band and a wrap without offset 0; the tempo delay and the comb their line capacity; the freeverb reverb, unless frozen, its core's sleep span plus its pre-delay line; every other mode `None`.

#### Scenario: Converted songs unchanged
- **WHEN** the three converted TBL songs render with and without the skip
- **THEN** they match within one 16-bit step

#### Scenario: Freeverb memory
- **WHEN** a freeverb reverb is asked `quiet_after()`
- **THEN** it answers at least 8192 frames plus its pre-delay line, and `None` while frozen

### Requirement: Woken effects resume in step
EQ 3band's `wake` SHALL restart its gain ramp from the parameters as set while it slept; the comb's SHALL advance its sweep by the frames skipped.

#### Scenario: Woken EQ
- **WHEN** an EQ 3band sleeps through a gain change and wakes before the next one
- **THEN** its next block ramps exactly as if it had run

#### Scenario: Woken comb
- **WHEN** a comb with a sweep sleeps 5000 frames and wakes
- **THEN** its output equals a comb that ran those 5000 frames of silence

### Requirement: Clocked filter latched at note-on
A clocked slot SHALL take its filter settings (base and amount after the retrigger byte, decay, damping) at a non-legato note-on and keep them until the next one.

#### Scenario: Retrigger byte changed mid-note
- **WHEN** the retrigger byte changes while a clocked note sounds
- **THEN** the note's filter is unchanged, and the next non-legato note-on takes the new value

### Requirement: Functional names
Parameter names, option values, cargo features, module ids, effect modes and the synth's documentation sections SHALL
be named by what they do, and so SHALL the DAW's user-facing strings. They SHALL NOT name a source synth or its maker
(V2, Farbrausch, Tunefish, WaveSabre, Ronan, Moog, cauldron). Source names SHALL appear only where the subject is the
source: converter commands, parity against a reference, provenance tables and song credits.

#### Scenario: No source names in the parameter table
- **WHEN** the parameter names, option values and feature names of the instrument are listed
- **THEN** no word token (split on non-letters) is `v2`, `tf`, `ws`, `farbrausch`, `tunefish`, `wavesabre`, `ronan`, `moog`
  or `cauldron` (case-insensitive)

#### Scenario: Provenance kept
- **WHEN** the provenance table of the synth documentation is read
- **THEN** each setting, law and quirk that came from a source synth names that source and the original module

### Requirement: Shared state-variable filter
The state-variable filter SHALL be one algorithm whose coefficients come from per-slot settings: response (low-pass,
band-pass, high-pass, notch, all-pass; chosen with the filter type), resonance form (input-scaled or feedback-scaled), oversampling form
(interpolated input with averaged output, or held input with last output) and cutoff/resonance law.

#### Scenario: Forms reproduce the earlier filters
- **WHEN** white noise runs through each earlier state-variable filter type and through the settings that replace it
- **THEN** the octave-band levels agree within 0.5 dB and the 3 dB cutoff point within 3 %

#### Scenario: Forms combine freely
- **WHEN** a slot uses the feedback resonance form with the Hz cutoff law and an all-pass response at zero resonance
- **THEN** the patch loads, and white noise through it keeps every octave-band level within 0.5 dB

### Requirement: Shared 4-pole filter
The 4-pole filter SHALL be one algorithm with per-slot settings: response (low-pass, high-pass;
chosen with the filter type), feedback taken before
or after the cubic clip, 1× or 2× oversampling, and coefficient law. The settings replacing the earlier ladder and the
earlier 4-pole types SHALL match them within 0.5 dB per octave band on white noise.

#### Scenario: High-pass on the Hz law
- **WHEN** a slot uses the 4-pole filter with the Hz law, high-pass response (the input minus the low-pass), at 1 kHz
- **THEN** the level at 10 kHz is within 1 dB of the input and the level at 100 Hz at least 6 dB below it

### Requirement: Envelope settings
Every envelope SHALL have these parameters:
- a time law: `seconds` (closed-form times), `rate` (log-rate steps) or `frame` (per-frame steps at 344.5 frames/s)
- a decay and release curve: quadratic, power with slope, or exponential
- a sustain slope (the level falls or rises while held; the centre keeps it constant)
- an amplify factor

The attack SHALL be linear in level for every combination.

#### Scenario: Sustain slope on a seconds-law envelope
- **WHEN** a seconds-law envelope with quadratic curves has its sustain slope below the centre and a note is held
- **THEN** the level falls during sustain, and at the centre slope it stays at the sustain level

#### Scenario: Earlier modes as settings
- **WHEN** a patch sets the settings that replace an earlier envelope mode
- **THEN** the envelope reaches each stage boundary within one control block of that mode, and its level differs by at most 0.5 dB above −60 dB

### Requirement: LFO settings
Every LFO SHALL offer the shapes sine, ramp up, ramp down, square/pulse, triangle, stepped random and sample-and-hold.
Its parameters SHALL be polarity (positive, negative, bipolar), one-shot (one cycle, then hold), start phase at key
sync, and rate law (Hz, or per-frame).

#### Scenario: Sample-and-hold on a Hz-law LFO
- **WHEN** a Hz-law LFO at 4 Hz uses sample-and-hold and drives filter cutoff
- **THEN** the cutoff takes a new value at each cycle start and holds it for 0.25 s

### Requirement: Slot combine operation
Every oscillator slot SHALL have a combine parameter: add to the running sum of the earlier slots, multiply the sum
(ring), or replace it. An added slot SHALL pan by its slot pan and the voice pan; multiply and replace SHALL act on
both channels of the sum.

#### Scenario: Ring on a band-limited slot
- **WHEN** slot 1 is a PM sine at 440 Hz and slot 2 a band-limited saw at 110 Hz set to multiply
- **THEN** the output holds the products of the two (energy at 440 ± 110 Hz); the carrier does not pass on its own (at
  least 12 dB below the same slots added), and 110 Hz holds only the products of the saw's third and fifth harmonics
  with the carrier

### Requirement: Voice output settings
The voice output stage SHALL be set by these parameters:
- amplitude source: master × velocity × envelope 1, or envelope 1 only
- pan stage: per slot, or once after the voice distortion on a mono sum
- output DC filter: off or on

There SHALL be no separate voice path for converted songs.

#### Scenario: Pan after distortion
- **WHEN** the pan stage is set to after the distortion and the voice pan is hard left
- **THEN** the right channel is silent and the left carries the distorted mono sum

### Requirement: Quirk flags
Behaviour that only reproduces particular source builds SHALL be a quirk flag of the instrument, named by function, and
SHALL NOT be a hidden branch of a shared algorithm. A quirk SHALL exist only if a converted song leaves the parity
tolerance without it. Each flag SHALL be documented with its source, the format versions that need it and the songs
that use it. Each flag SHALL be a cargo feature.

#### Scenario: A quirk is listed with its songs
- **WHEN** the documentation of the integer-FM quirk is read
- **THEN** it names its source, the format versions it applies to and the corpus songs that move past tolerance without it (if the survey keeps it; a dropped quirk is listed as dropped with the survey's numbers)

#### Scenario: Quirk disabled
- **WHEN** a patch sets a quirk flag and the build lacks that feature
- **THEN** loading fails with an unsupported-module error naming the quirk

### Requirement: Point oscillator
With feature `us-osc-points`, an oscillator slot of type `points` SHALL build a 256-sample cycle from the fixed points
(0, 0) and (1, 0) and 1–4 user points. Each user point's offset is placed in the span left after the previous point
(clamped to 0.01–0.99 of it) and its value is `2p − 1`. Each segment is interpolated by its own mode: step, linear or
spline.

#### Scenario: Step cycle
- **WHEN** a `points` slot has one point at offset 0.5, value 1, step mode on both segments
- **THEN** the cycle is 0 for the first half and 1 for the second half (within one sample of the boundary)

### Requirement: Point oscillator playback
A `points` cycle SHALL be rebuilt at a control segment only when a point input (after the mod matrix) changed. It
SHALL be played with the wavetable slot's layers, left/right detune, spread, drive and hard clip, reading without
interpolation.

#### Scenario: Modulated point
- **WHEN** an LFO Scale slot targets the slot's point-1 value
- **THEN** the cycle changes between control segments as the LFO moves, and stays fixed while the LFO output is constant

#### Scenario: Missing module
- **WHEN** a patch with a `points` slot loads in a build without `us-osc-points`
- **THEN** the load fails with `UnsupportedModule` naming the point oscillator

### Requirement: Tunefish 3 decimation stage
A `points` or `pad` slot SHALL always render through Tunefish 3's decimation stage: at k times the song rate, k the
smallest integer with `k·rate ≥ 64·f` for the voice's note frequency f (at most 8), its output scaled by the slot gain
passes two cascaded ladder low-passes at 12.83 kHz designed at that rate, and k samples are averaged, also when k = 1.

#### Scenario: High note
- **WHEN** a sawtooth-like `points` cycle plays note 100
- **THEN** its octave bands above 10 kHz are lower than those of the same cycle read once per frame without the stage

#### Scenario: Low note still filtered
- **WHEN** a `points` square plays note 45 (k = 1)
- **THEN** its 16 kHz octave band is lower than without the stage

### Requirement: PAD oscillator
With feature `us-osc-pad`, an oscillator slot of type `pad` SHALL play a static PADsynth table of 2^16 samples. The
table is built once at load from the slot's bandwidth, bandwidth scale, damp, harmonics and profile (single, detune,
spread, gauss). These values SHALL be structural (`set_param` ignores them). The slot advances `f/260` table periods
per sample, so its fundamental sounds at 0.499·f, without interpolation, with left/right detune, drive and hard clip.

#### Scenario: Spectrum
- **WHEN** a `pad` slot with 8 harmonics and the gauss profile plays note 69
- **THEN** the output's spectrum peaks at the harmonics of 219.6 Hz (440 · 0.499), and each peak's width grows with the bandwidth value

#### Scenario: Automation ignored
- **WHEN** `set_param` changes a `pad` slot's bandwidth during playback
- **THEN** the output is identical to the output without that call

### Requirement: Shared PAD table memory
A PAD table SHALL be allocated once per `pad` slot and instrument, not per voice. `need` SHALL count it once.

#### Scenario: Need with 16 voices
- **WHEN** `need` is asked for a patch with one `pad` slot and capacity 16, then capacity 1
- **THEN** both results differ only by the per-voice size of 15 voices, and each includes one table of 2^16 f32

### Requirement: New oscillator type storage
The `points` and `pad` types SHALL be stored at free positions of the expanded oscillator-type choice. Stored values
of the existing types keep their meaning. The point oscillator's point count, interpolation modes, layers, layer
detune and layer spread SHALL be appended parameters after the current last index.

#### Scenario: Existing songs unchanged
- **WHEN** the existing golden songs are rendered in a build with `us-osc-points` and `us-osc-pad`
- **THEN** they are byte-identical to their goldens

#### Scenario: Round trip
- **WHEN** a patch with a `points` slot and a `pad` slot is encoded and parsed again
- **THEN** both slot types, the point count, every interpolation mode and the layer settings decode unchanged

### Requirement: Tunefish 3 filter form
A filter slot's `form` set to 1 on the RBJ high-pass, RBJ band-pass and notch types SHALL select Tunefish 3's form:
the band-pass with `α = sin ω0/2 · √((A + 1/A)(1/BW − 1) + 2)`, `A = 1.059253725`, `BW = 1 − clamp(q, 0, 0.95)` and
constant skirt gain; the high-pass with resonance up to 0.95; the notch as an RBJ notch with the same α and no latency.
Form 0 SHALL keep these types as they were. The form SHALL require `us-flt-feedback`.

#### Scenario: Wide band-pass
- **WHEN** an RBJ band-pass at 1 kHz with form 1 and resonance 0.5 plays a 1 kHz sine
- **THEN** its gain is `1/√((A + 1/A) + 2)` within 1 %

#### Scenario: Stored songs unchanged
- **WHEN** the project's songs and patches load
- **THEN** none sets form on an RBJ or notch slot, and the reference songs render bit-identically

#### Scenario: Build without the forms
- **WHEN** a build without `us-flt-feedback` loads a patch with an RBJ slot in form 1
- **THEN** the load fails with `UnsupportedModule` naming `us-flt-feedback`

### Requirement: One-tap chorus mode
The chorus kind SHALL have a second mode, one tap, behind `us-fx-chorus-tf3`: per channel `y = x + gain·x[n − d]`,
`d = 1 + (sin(t·π·rate/sr) + 1)/2 · fi(50·depth²)` ms rounded to frames, on the song's absolute frame. A chorus
stored before this mode existed SHALL keep mode 0.

#### Scenario: Impulse
- **WHEN** an impulse passes the one-tap chorus at depth 0 and gain 0.5
- **THEN** the dry impulse is unchanged and one copy at 0.5 follows 44 frames later at 44.1 kHz

#### Scenario: Split render
- **WHEN** a render is split into calls of other sizes
- **THEN** the output is identical

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
