# Spec Delta

## ADDED Requirements

### Requirement: V2 parameters are appended
The parameters V2 needs SHALL be appended after the last existing parameter index (183), in one fixed order frozen
with this change. Their defaults SHALL leave every existing patch unchanged: a chunk without them SHALL load and render
bit-identically to before.

#### Scenario: Old chunk unchanged
- **WHEN** a chunk written before this change (indices ≤ 183) is loaded
- **THEN** every self-golden hash recorded before this change still matches

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

### Requirement: V2 oscillator slot type
An oscillator slot SHALL offer a V2 type with the modes saw↔triangle (colour morphs saw to triangle), pulse (colour =
width), sine, coloured noise, FM (phase modulated by the running sum of the earlier slots) and aux A/B input. A V2 slot
SHALL read its transpose, detune, colour and volume with V2's laws (value = V2 raw value / 128).

#### Scenario: Pulse width
- **WHEN** a V2 slot in pulse mode plays note 57 with colour at the centre
- **THEN** the output is a square wave at 220 Hz with odd harmonics only (even harmonics at least 30 dB lower)

#### Scenario: Aux input mode
- **WHEN** a V2 slot in aux A mode receives a 440 Hz sine on the device's aux input pair 1
- **THEN** the slot's output is that sine × its volume

### Requirement: V2 ring mode
Each V2 oscillator mode SHALL support ring mode, which multiplies the slot into the running sum of the earlier slots
instead of adding it.

#### Scenario: Ring mode
- **WHEN** slot 1 is a V2 sine and slot 2 a V2 sine one octave higher in ring mode
- **THEN** the output holds the sum and difference frequencies and not the two input tones

### Requirement: V2 filters and routing
A filter slot SHALL offer the V2 state-variable filter (low-pass, band-pass, high-pass, notch, all-pass) with V2's
cutoff and resonance laws, and the V2 Moog filter (low-pass, high-pass). A voice routing parameter SHALL run filter
slots 1 and 2 serially (the default) or in parallel, mixed by a balance parameter. Slots 3 and 4 SHALL follow in series.

#### Scenario: Parallel balance
- **WHEN** slot 1 is a V2 low-pass at 200 Hz and slot 2 a V2 high-pass at 5 kHz in parallel at full balance towards slot 2
- **THEN** the output equals slot 2's output alone

#### Scenario: All-pass
- **WHEN** white noise runs through a V2 all-pass
- **THEN** the octave-band levels are within 0.5 dB of the input

### Requirement: Voice distortion stage
A voice SHALL have one distortion stage after the filters with the modes off, overdrive, clip, bit crush, decimate,
and the filter modes low-pass, band-pass, high-pass, notch, all-pass and Moog low-pass, with input gain and two mode
parameters using V2's laws. Off SHALL be the default, and an off stage SHALL not change the signal.

#### Scenario: Overdrive
- **WHEN** a full-scale sine runs through overdrive at high input gain
- **THEN** the output peak stays at or below 1 and odd harmonics appear

### Requirement: Channel stage
An instrument SHALL have one channel stage after the voice sum with V2's modules: channel volume, bass boost, channel
distortion (the voice distortion modes), chorus/flanger, channel compressor, and the order of distortion and chorus. It
SHALL have four send levels (reverb, delay, aux A, aux B) whose signals leave the device as extra output pairs. The
defaults SHALL make the stage transparent, with every send at zero.

#### Scenario: Transparent default
- **WHEN** a patch leaves the channel stage at its defaults
- **THEN** the main output is bit-identical to the output without the stage, and the send outputs are silent

#### Scenario: Reverb send
- **WHEN** the reverb send is at full level
- **THEN** the reverb output pair carries the channel signal after the stage

### Requirement: Ronan speech stage
An instrument SHALL have an optional speech stage on the voice sum, before the channel stage. Its chunk SHALL hold the
phoneme text. Each note-on SHALL advance the text by one syllable, as V2 does, and controllers 1–7 SHALL steer it as in
V2. Off SHALL be the default.

#### Scenario: Speech text
- **WHEN** an instrument with the speech stage on and the text of fr-08's speech channel plays that channel's notes
- **THEN** its output passes the parity metrics against the reference render of that channel

### Requirement: V2 module features
Each V2 module (oscillator type, filters, routing, voice distortion, envelope mode, LFO features, channel stage, speech,
aux input and output pairs) SHALL be a `us-*` cargo feature. A patch that needs a disabled one SHALL fail at load with
an unsupported-module error naming it, and the song feature list SHALL name it.

#### Scenario: Speech disabled
- **WHEN** a patch with the speech stage on loads in a build without the speech feature
- **THEN** loading fails with an unsupported-module error naming the speech module

## MODIFIED Requirements

### Requirement: Device kind and chunk
The instrument SHALL be `synth-mix` device kind 64. Its chunk SHALL hold a format version, the parameter values that
differ from their defaults as index/value pairs in strictly increasing index order, a sample section, and (format
version 2 only) a speech text section, with no bytes after it. Parameters not in the chunk SHALL take their defaults.
A chunk without speech text SHALL be written as format version 1.

#### Scenario: Sparse chunk
- **WHEN** a chunk sets only oscillator slot 1 type and level
- **THEN** the instrument loads, every other parameter has its default value, and a note renders sound

#### Scenario: Malformed chunk
- **WHEN** a chunk is truncated at any byte, has an unknown version, a parameter index out of range, repeated or unsorted indices, a non-finite or out-of-range value, or bytes after its last section
- **THEN** loading returns an error and does not panic

#### Scenario: Speech text section
- **WHEN** a patch with speech text is encoded and loaded again
- **THEN** the chunk has format version 2, the text round-trips byte for byte, and the same patch without text encodes as version 1

### Requirement: Mod matrix
The voice SHALL have a 16-slot mod matrix: source (envelope 1–4, LFO 1–2, mod wheel, velocity, key, and the
controllers breath, CC 3–6 and volume), target, amount and operation. A Scale slot SHALL multiply the target's
normalized value (several Scale slots multiply together). An Offset slot SHALL add amount × source in the target's
physical unit after mapping (several add together). An Add slot SHALL add amount × source to the normalized value of its
parameter target before the law, clamping to 0..1 after each slot in slot order (V2's rule: amount −1..1, source
0..1). Slots 13–16 SHALL be
appended parameters, and their default SHALL be no source.

#### Scenario: Scale composition
- **WHEN** two Scale slots aim at filter 1 cutoff with results 0.5 and 0.5
- **THEN** the cutoff mapping receives 0.25 × the base normalized value

#### Scenario: Pitch offset
- **WHEN** an Offset slot routes a fully open envelope to pitch with amount +12
- **THEN** the voice plays one octave above the note while the envelope is at 1

#### Scenario: V2 add with clamp
- **WHEN** a V2 filter's cutoff value is 0.75 and two Add slots route velocity 1.0 with amounts +0.5 and −0.125
- **THEN** the cutoff law receives 0.875 (0.75 + 0.5 clamps to 1.0, then 1.0 − 0.125), as V2's per-entry clamp gives

#### Scenario: Controller source
- **WHEN** an Add slot routes the volume controller to the channel volume and the song sets controller 7 to 0
- **THEN** the channel volume law receives its base value minus the amount

### Requirement: Modulation targets
Targets SHALL include voice pitch (all slots); each oscillator slot's pitch, gain, mix level, pan, phase-mod amount
and type-specific shape parameters; noise level and band; each filter slot's cutoff and resonance; envelope decay; LFO
depth; each mod slot's own amount; voice pan and level. The target ids frozen with chunk v1 SHALL keep their meaning.
New ids SHALL be appended for every appended V2 parameter that V2 can modulate, including the channel stage
parameters. Channel stage targets SHALL be evaluated from the newest sounding voice of the instrument, as V2 does.

#### Scenario: Mod slot depth
- **WHEN** slot 1 routes LFO 1 to filter cutoff and slot 2 routes envelope 2 to slot 1's amount (Scale)
- **THEN** the LFO's effect on the cutoff follows envelope 2

#### Scenario: Channel target from the newest voice
- **WHEN** an Add slot routes velocity to the reverb send, and a note at velocity 127 is followed by one at velocity 32
- **THEN** after the second note-on the reverb send follows velocity 32

### Requirement: Envelopes
A voice SHALL have 4 ADSR envelopes, and SHALL end when envelope 1 finishes. Envelope 1 SHALL scale the voice output
after the filters unless the patch sets it to act only through mod-matrix routes. Each envelope SHALL use one of three
curve modes: linear attack with quadratic decay/release (WaveSabre), rate-based log steps with a slope (Tunefish), or
V2's envelope (attack, decay, sustain level, sustain time that lets the level fall or rise while held, release, and
amplify). Times SHALL be in seconds, independent of sample rate.

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
- **WHEN** a V2-mode envelope has sustain time below the centre and a note is held
- **THEN** the level falls during the sustain stage, and at the centre value it stays constant

### Requirement: LFOs
A voice SHALL have 2 LFOs with rate, depth and shape (sine, ramp up, ramp down, square, stepped random, triangle,
sample-and-hold), each either restarting at note-on or free-running across notes. An LFO SHALL also offer V2's
envelope mode (one cycle, then hold), a start phase, and polarity (positive, negative, bipolar). A Scale slot SHALL see
an LFO as `1 − d + d·shape` (unipolar, d = depth²) and an Offset slot as `d·(2·shape − 1)` (bipolar). An Add slot SHALL
see V2's value for the polarity (0..1, −1..0, or −1..1) × amplify.

#### Scenario: Key sync
- **WHEN** an LFO is set to restart at note-on and two notes start 0.3 s apart
- **THEN** both voices' LFO outputs at their own note-on equal the shape's start value

#### Scenario: Vibrato as an offset
- **WHEN** a sine LFO at depth 1 drives an Offset slot on pitch with amount 0.5
- **THEN** the pitch swings between −0.5 and +0.5 semitones around the note

#### Scenario: Envelope mode
- **WHEN** a saw LFO at 2 Hz in envelope mode is key-synced and a note is held for 2 s
- **THEN** the LFO completes one ramp in 0.5 s and then holds its end value
