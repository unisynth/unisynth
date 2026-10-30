# Spec Delta

## MODIFIED Requirements

### Requirement: Slot racks
The instrument editor SHALL show its fixed slot groups (oscillators, filters, envelopes, LFOs, mod slots) as blocks in
a fixed layout in which every slot is always shown at its own position, with a used/maximum count per block. A slot that
is not set (type Off, or every value at its default) SHALL start collapsed to one header line with its type control; a
set slot SHALL start expanded. Setting a type in a collapsed header SHALL set and expand the slot without moving any
other slot. Envelope 1 (amplitude) SHALL be marked fixed. The filter block SHALL show the serial order. The mod matrix
SHALL be a table of 12 rows (source, target, operation, amount) with rows whose source is None dimmed.

#### Scenario: Turning on filter 3
- **WHEN** an instrument uses filter 1 only and the user sets filter 3's type in its collapsed header
- **THEN** filter 3 expands at the third position, filter 2 stays collapsed and Off, and the count reads 2/4

#### Scenario: Stable positions
- **WHEN** the user turns oscillator 2 on and off again
- **THEN** no other slot changes position or number and nothing is renumbered in the song

### Requirement: Slot remove and move
Clearing a slot (✕) SHALL reset that slot to off or defaults in place; other slots SHALL NOT move. Clearing SHALL
clear the mod slots that targeted the cleared slot and remove its automation lanes. Only filters SHALL be reordered: a
handle between two neighbouring filters SHALL swap their values. Each SHALL be one undo step; a swap SHALL rewrite the
mod-matrix targets and automation lanes of both filters.

#### Scenario: Clear in place
- **WHEN** oscillators 1 and 2 are on, mod slot 1 targets `osc2.gain`, and the user clears oscillator 1
- **THEN** oscillator 1 is Off at its position, oscillator 2 keeps its number and values, and mod slot 1 still targets
  `osc2.gain`

#### Scenario: Reorder keeps modulation
- **WHEN** env 2 modulates `filt1.cutoff`, a lane automates `filt1.resonance`, and filters 1 and 2 are swapped
- **THEN** the mod slot targets `filt2.cutoff`, the lane targets `filt2.resonance`, and the sound is unchanged except for
  the new filter order

### Requirement: Live device graphics
Envelope curves SHALL be drawn as the engine plays them (per envelope mode) and SHALL redraw whenever a parameter they
depend on changes, by hand or by automation. A sampler slot SHALL show its sample's waveform with the start and loop
and SHALL offer its playback settings (loop mode, loop region, interpolation, reverse) as named controls, showing loop
start and length only for a custom loop that plays; dragging the start line or the loop edits those values in one
undo step. Every active oscillator SHALL show its waveform, every active filter its frequency response with the
cutoff marked, and every LFO its shape, each redrawn when a parameter it depends on changes.

#### Scenario: Turning release
- **WHEN** the user turns Env 1 release
- **THEN** the envelope curve changes while the knob moves

#### Scenario: Setting a loop on a sample
- **WHEN** the user sets loop to Repeat, region to Custom and drags the loop end on the waveform
- **THEN** loop start and length knobs appear and follow the drag, and Undo restores the previous loop

#### Scenario: Turning cutoff
- **WHEN** the user turns filter 1 cutoff
- **THEN** the response curve and its cutoff mark move while the knob moves

## ADDED Requirements

### Requirement: Collapsible blocks and slots
Every slot and every block of the instrument editor SHALL have a collapse/expand toggle exposed as a button with
`aria-expanded`. A block's collapsed state SHALL persist across track changes and reloads in browser storage and SHALL
NOT be saved in the song.

#### Scenario: Collapse survives a track change
- **WHEN** the user collapses the Mod matrix block, selects another track and selects the first again
- **THEN** the Mod matrix block is still collapsed and the saved song is unchanged

### Requirement: Block colours and routing hints
Each block (voice, oscillators, filters, envelopes, LFOs, mod matrix) SHALL have its own accent colour on set slots;
slots that are not set SHALL show no accent. Envelope and LFO slots SHALL show the targets that mod rows route them to,
or "unrouted" with their knobs dimmed. Mod rows SHALL mark their source's block colour. Text SHALL meet WCAG AA contrast.

#### Scenario: Routing hint
- **WHEN** mod slot 2 routes LFO 1 to `filt1.cutoff`
- **THEN** the LFO 1 header shows "→ filt1.cutoff" and mod row 2 carries the LFO block colour

### Requirement: Live per-voice displays
While the selected track's instrument sounds (playback, keyboard or MIDI audition), its screens SHALL show the
engine's voice state with one marker per voice: sampler playheads, envelope level dots and level bars, LFO phase dots.
Each voice SHALL keep one colour in every block and a note chip in the device header. Marker brightness SHALL follow the
voice's amplitude; released voices SHALL be shown faded.

#### Scenario: A chord on the sampler
- **WHEN** a four-note chord plays on a track whose oscillator 1 is a sampler
- **THEN** four chips show the notes, the waveform shows four playheads in those colours and each envelope shows four dots

#### Scenario: Silence
- **WHEN** all voices have ended
- **THEN** no markers or chips are shown and the screens show the static curves

### Requirement: Coinciding and excess voice markers
Markers of voices at the same position SHALL stack so each stays visible. At most 16 voices SHALL be drawn, the loudest;
beyond that the chip row SHALL show "+N".

#### Scenario: Chord at sustain
- **WHEN** four voices hold at the sustain level of envelope 1
- **THEN** four distinct dots are visible at the sustain point

#### Scenario: Many voices
- **WHEN** 24 voices sound
- **THEN** 16 voices are drawn and the chip row ends with "+8"

### Requirement: Live filter spectrum
While voices sound, each active filter screen SHALL show the spectrum of the filter's input in grey and of its output
in the filter block's colour, summed over the voices, and a tick per voice at that voice's modulated cutoff.

#### Scenario: Low-pass at work
- **WHEN** a bright saw chord plays through a low-pass filter at 300 Hz
- **THEN** the grey input shows harmonics above 300 Hz that the coloured output does not

#### Scenario: Modulated cutoff
- **WHEN** an LFO modulates filter 1's cutoff during a chord
- **THEN** the cutoff ticks of the voices move over time, apart from each other when their LFO phases differ

### Requirement: Live view toggle
A Live toggle in the instrument header SHALL turn the live per-voice view on and off. It is on by default and remembered in the browser, not in the song. While it is off, the editor watches no track, so the engine does no probe work.

#### Scenario: Live view off
- **WHEN** the user turns Live off in the instrument header and plays notes
- **THEN** the editor watches no track, status reports carry no probe data, no voice chips or markers are drawn, and the choice is kept after a reload
