# Spec Delta

## ADDED Requirements

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

- FROM: `### Requirement: Cauldron oscillator slot type`
- TO: `### Requirement: Clocked oscillator slot type`

- FROM: `### Requirement: Cauldron voices and version`
- TO: `### Requirement: Clocked voices and version`

- FROM: `### Requirement: Cauldron note-on phase`
- TO: `### Requirement: Clocked note-on phase`

- FROM: `### Requirement: Cauldron filter`
- TO: `### Requirement: Clocked filter`

- FROM: `### Requirement: Cauldron per-note compounding`
- TO: `### Requirement: Clocked per-note compounding`

- FROM: `### Requirement: Cauldron phrase position`
- TO: `### Requirement: Clocked phrase position`

- FROM: `### Requirement: Cauldron filter note-on`
- TO: `### Requirement: Clocked filter note-on`

- FROM: `### Requirement: Cauldron playback rate`
- TO: `### Requirement: Clocked playback rate`

- FROM: `### Requirement: Cauldron word byte`
- TO: `### Requirement: Clocked word byte`

- FROM: `### Requirement: Cauldron phrase clock`
- TO: `### Requirement: Clocked phrase clock`

- FROM: `### Requirement: Cauldron voice movement`
- TO: `### Requirement: Clocked voice movement`

- FROM: `### Requirement: Cauldron type storage`
- TO: `### Requirement: Clocked type storage`

- FROM: `### Requirement: Cauldron filter latched at note-on`
- TO: `### Requirement: Clocked filter latched at note-on`

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
- **THEN** loading returns an error and does not panic (the checked build; a baked build trusts the song it was
  generated with and is not required to check)

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

### Requirement: Clocked filter latched at note-on
A clocked slot SHALL take its filter settings (base and amount after the retrigger byte, decay, damping) at a non-legato note-on and keep them until the next one.

#### Scenario: Retrigger byte changed mid-note
- **WHEN** the retrigger byte changes while a clocked note sounds
- **THEN** the note's filter is unchanged, and the next non-legato note-on takes the new value

## REMOVED Requirements

### Requirement: V2 parameters are appended
**Reason**: The layout is redesigned around shared settings with functional names; the old layout is no longer frozen.
**Migration**: Chunks and song files in the old layout are converted by the host migration (synth-convert spec,
"Old-layout migration"); converted songs are re-converted from their sources.
