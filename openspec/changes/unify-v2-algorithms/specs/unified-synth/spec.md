# Spec Delta

## ADDED Requirements

### Requirement: Functional names
Parameter names, option values, cargo features, module ids, effect modes and the synth's documentation sections SHALL
be named by what they do. They SHALL NOT name a source synth (V2, Tunefish, WaveSabre). Source names SHALL appear only
where the subject is the source: converter commands, parity against a reference and provenance tables.

#### Scenario: No source names in the parameter table
- **WHEN** the parameter names, option values and feature names of the instrument are listed
- **THEN** none contains `v2`, `tunefish`, `tf`, `wavesabre`, `ws`, `ronan` or `moog` (case-insensitive)

#### Scenario: Provenance kept
- **WHEN** the provenance table of the synth documentation is read
- **THEN** each setting, law and quirk that came from a source synth names that source and the original module

### Requirement: Shared state-variable filter
The state-variable filter SHALL be one algorithm whose coefficients come from per-slot settings: response (low-pass,
band-pass, high-pass, notch, all-pass), resonance form (input-scaled or feedback-scaled), oversampling form
(interpolated input with averaged output, or held input with last output) and cutoff/resonance law.

#### Scenario: Forms reproduce the earlier filters
- **WHEN** white noise runs through each earlier state-variable filter type and through the settings that replace it
- **THEN** the octave-band levels agree within 0.5 dB and the 3 dB cutoff point within 3 %

#### Scenario: Forms combine freely
- **WHEN** a slot uses the feedback resonance form with the Hz cutoff law and an all-pass response at zero resonance
- **THEN** the patch loads, and white noise through it keeps every octave-band level within 0.5 dB

### Requirement: Shared 4-pole filter
The 4-pole filter SHALL be one algorithm with per-slot settings: response (low-pass, high-pass), feedback taken before
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
- **THEN** the output holds the products of the two (energy at 440 ± 110 Hz) and no 110 Hz fundamental

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
- **THEN** it names its source (V2 before format 6), the format versions it applies to and the corpus songs (for example kkrieger) that leave tolerance without it

#### Scenario: Quirk disabled
- **WHEN** a patch sets a quirk flag and the build lacks that feature
- **THEN** loading fails with an unsupported-module error naming the quirk

## RENAMED Requirements

- FROM: `### Requirement: V2 oscillator slot type`
- TO: `### Requirement: Naive oscillator generators`

- FROM: `### Requirement: V2 ring mode`
- TO: `### Requirement: Ring combine`

- FROM: `### Requirement: V2 filters and routing`
- TO: `### Requirement: Filter routing`

- FROM: `### Requirement: V2 module features`
- TO: `### Requirement: Shared-algorithm module features`

- FROM: `### Requirement: Ronan speech stage`
- TO: `### Requirement: Speech stage`

## MODIFIED Requirements

### Requirement: Device kind and chunk
The instrument SHALL be `synth-mix` device kind 64. Its chunk SHALL hold a format version, the parameter values that
differ from their defaults as index/value pairs in strictly increasing index order, a sample section, and (format
version 4 only) a speech text section, with no bytes after it. Parameters not in the chunk SHALL take their defaults.
A chunk without speech text SHALL be written as format version 3. The player SHALL reject format versions 1 and 2 (the
layout before this change) with an error; the host migration converts them.

#### Scenario: Sparse chunk
- **WHEN** a chunk sets only oscillator slot 1 type and level
- **THEN** the instrument loads, every other parameter has its default value, and a note renders sound

#### Scenario: Malformed chunk
- **WHEN** a chunk is truncated at any byte, has an unknown version, a parameter index out of range, repeated or unsorted indices, a non-finite or out-of-range value, or bytes after its last section
- **THEN** loading returns an error and does not panic

#### Scenario: Speech text section
- **WHEN** a patch with speech text is encoded and loaded again
- **THEN** the chunk has format version 4, the text round-trips byte for byte, and the same patch without text encodes as version 3

#### Scenario: Old layout rejected by the player
- **WHEN** the player loads a format version 1 chunk
- **THEN** loading returns an unknown-version error and does not panic

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

### Requirement: Speech stage
An instrument SHALL have an optional formant speech stage on the voice sum, before the channel strip. Its chunk SHALL
hold the phoneme text. Each note-on SHALL advance the text by one syllable, and controllers 1–7 SHALL steer it (text
select and rate, pitch). Off SHALL be the default.

#### Scenario: Speech text
- **WHEN** an instrument with the speech stage on and the text of fr-08's speech channel plays that channel's notes
- **THEN** its output passes the parity metrics against the reference render of that channel

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

## REMOVED Requirements

### Requirement: V2 parameters are appended
**Reason**: The layout is redesigned around shared settings with functional names; the old layout is no longer frozen.
**Migration**: Chunks and song files in the old layout are converted by the host migration (synth-convert spec,
"Old-layout migration"); converted songs are re-converted from their sources.
