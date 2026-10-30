# Spec Delta

## ADDED Requirements

### Requirement: Cauldron oscillator slot type
An oscillator slot SHALL offer the type **cauldron** behind the feature `us-osc-cauldron`. It produces the
mini-cauldron waves (saw, square, spike, pulse, noise, fold) on a 16-bit phase, with naive (not band-limited) output.
Its waveform values SHALL equal the original renderer's for the same phase, pitch increment and frame count, scaled
by 1/32768.

#### Scenario: Saw matches the original
- **WHEN** a cauldron slot with wave saw, voices 0 plays a note whose increment equals the original pitch word 799 at the original rate
- **THEN** its output over one second equals the original oscillator's 16-bit saw scaled by 1/32768, sample for sample

### Requirement: Cauldron voices and version
The cauldron slot's **voices** field (0–10) SHALL add the original second saw at a ×1.02 increment, or the spike's
detuned copy. The **version** field SHALL select v1 or v2 behaviour. Version 2 uses the spike without sign handling,
the 16-bit truncated detune offset, and a clamp of the oscillator output to full scale. Version 2 in a glide SHALL
hold a word above 32767 as its signed value for the second saw and the spike, as the original's glide register.

#### Scenario: Glide register
- **WHEN** a version-2-in-a-glide slot with voices 1 plays word 51199 for one frame
- **THEN** its phase steps by 51199 and its second saw by −14624 (not 52223) of 65536

#### Scenario: Spike wave
- **WHEN** a version-1 cauldron slot plays wave spike with voices 3
- **THEN** each half cycle is a narrow peak of 32767·u⁷ shape, plus a second copy detuned by +3/128 that drifts with the frames since the phrase's first note

### Requirement: Cauldron note-on phase
A non-legato note-on SHALL set the cauldron phase to −32766/32768 of a cycle. It SHALL keep the second voice's phase,
unless the slot's phase byte (b ≥ 1) sets it to `(b − 1)/255` of a cycle. A legato note SHALL keep both.

#### Scenario: Phase byte
- **WHEN** a non-legato note-on finds the phase byte 129
- **THEN** the second voice starts at 128/255 of a cycle

#### Scenario: Legato keeps the phase
- **WHEN** a legato note follows a sounding note on a mono cauldron instrument
- **THEN** the phase continues without a jump

### Requirement: Cauldron filter
The cauldron slot SHALL run the original resonant low-pass on its output when its filter base or amount is above 0,
with base, envelope amount, decay per phrase frame and damping in the original's phrase-frame units. At one phrase
frame per output frame its output SHALL equal the original's within 1e-4 of the peak.

#### Scenario: Matches the original filter
- **WHEN** a cauldron slot with the filter on plays the reference test cases at one phrase frame per output frame
- **THEN** its output is within 1e-4 of the peak of the original filter's output, including a retrigger

#### Scenario: Off by default
- **WHEN** the filter base and amount are 0
- **THEN** the slot's output is unfiltered and existing goldens are unchanged

### Requirement: Cauldron per-note compounding
The cauldron slot SHALL compound its filter base and amount per note: times its compounding factor once per count
in the note's retrigger byte (`2k + accent`), each product rounded to f32, then times the accent (×4 version 1, ×2
version 2).

#### Scenario: Fifth retrigger with accent
- **WHEN** a version-1 note has the retrigger byte 11
- **THEN** its base and amount are the stored ones times the factor five times over, times 4

### Requirement: Cauldron phrase position
A non-legato cauldron note-on SHALL set the phrase frame counter to `(velocity − 1)` eighths of a 3530-frame step.

#### Scenario: Step start
- **WHEN** a note-on has velocity 41
- **THEN** the phrase frame counter is 5 × 3530

### Requirement: Cauldron filter note-on
A non-legato note-on SHALL restart the cauldron filter as the original's retrigger does: output memory 0, envelope
part = the amount, the rest of the state running on. A legato note SHALL keep it running. Since the restart zeroes
the output, a restart that cuts a sounding note SHALL carry the step from its last output frame to the new note's
first into the new note and fade it out linearly over 1 ms; a note from silence (below −80 dB) carries nothing.

#### Scenario: A sounding note cut by a restart
- **WHEN** a one-voice cauldron instrument restarts while its filtered note sounds
- **THEN** the output does not drop to 0 in one frame: the step at the restart is no larger than twice the wave's own largest step

#### Scenario: Rising cutoff
- **WHEN** the decay is above 1 and a legato note follows
- **THEN** the envelope part of the cutoff keeps growing across the note

### Requirement: Cauldron playback rate

A voice with a cauldron slot SHALL treat its voice-wide transpose and bend as a playback rate: the slot's phrase
clock and the voice's envelopes SHALL advance `2^((transpose + bend)/12)` times as fast, leaving the pitch word
unchanged, as an XM pitch slide resamples the precalc sample.

#### Scenario: An octave up plays the phrase twice as fast
- **WHEN** a cauldron voice with a rising filter is transposed (or bent) +12 semitones
- **THEN** after 1100 output frames its filter envelope has grown 2200 phrase frames, and a 200 ms release after a
  note-off ends in about half the time

### Requirement: Cauldron word byte
A cauldron slot SHALL read its fixed-frequency field as a word byte: b ≥ 1 makes its pitch word the frequency's word
rounded to an integer plus `b − 128`; 0 SHALL leave the frequency's word unchanged.

#### Scenario: Correction
- **WHEN** the frequency asks for word 51199.3 and the word byte is 125
- **THEN** the slot steps by 51196 of 65536 per phrase frame

### Requirement: Cauldron phrase clock
The cauldron slot (oscillator and filter) SHALL run at its phrase clock and be resampled to the output rate, so its
naive waveforms alias as the original's do at its own rate.

#### Scenario: Half rate
- **WHEN** the slot runs at half a phrase frame per output frame
- **THEN** every second output sample is a phrase-rate sample and the others lie between their neighbours

### Requirement: Cauldron voice movement
In a voice with a cauldron slot, a mono slide SHALL be linear in frequency, and a timed envelope attack of 0 SHALL
be instant.

#### Scenario: Octave glide
- **WHEN** a cauldron voice slides up an octave
- **THEN** halfway through the slide its frequency is 1.5 times the start, not √2 times

### Requirement: Cauldron type storage
The cauldron type SHALL be stored at a free position of the expanded oscillator-type choice, so stored values of
existing types keep their meaning. A patch using it in a build without the feature SHALL fail at load naming the
module.

#### Scenario: Existing songs unchanged
- **WHEN** the existing golden songs are rendered in a build with `us-osc-cauldron`
- **THEN** they are byte-identical to their goldens

#### Scenario: Missing module
- **WHEN** a patch with a cauldron slot loads in a build without `us-osc-cauldron`
- **THEN** the load fails with `UnsupportedModule` naming the cauldron oscillator
