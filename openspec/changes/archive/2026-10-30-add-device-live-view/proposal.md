# Proposal: fixed-slot device panel with live, per-voice displays

## Why

The Unisynth device panel shows only the slots in use and packs them: adding, removing or reordering a slot moves the
other slots, so the panel never has a stable layout and the user loses their place. The panel also shows only static
values; while a note plays the user cannot see where the sampler is, which envelope stage each voice is in, what the
LFOs are doing or what a filter does to the signal. The reviewed mockup
(`openspec/changes/archive/2026-10-30-add-synth-daw/mockup/device-fixed-slots.html`) settles the layout, colours and live displays.

## What Changes

- **BREAKING (editor behaviour)** Slot racks become fixed slot blocks: every oscillator, filter, envelope, LFO and mod
  slot is always shown at its own position. A slot that is not set (type Off, or all defaults) starts collapsed to a
  one-line header with its type control; a set slot starts expanded. "+ Osc / + Filter / …" buttons are removed.
- ✕ resets one slot in place (later slots do not move up). Mod slots that targeted it are cleared and its automation
  lanes are removed, in one undo step. Reordering remains only for filters (serial order matters): a ⇅ handle swaps
  two neighbouring filters, with mod targets and lanes following.
- Every slot and every block (Oscillators, Filters, Envelopes, LFOs, Mod matrix) has a collapse/expand toggle; the state
  is editor state kept per block in browser storage, not in the song.
- The mod matrix is a 12-row table (source, target, operation, amount); rows with source None are dimmed. Envelope
  and LFO slots show what they route to, and dim their knobs when nothing routes them.
- Each block has its own accent colour (header strip, tint, knob arcs). Active slots get a display screen: oscillator
  waveform, filter response, envelope shape, LFO shape — all drawn from the current parameter values.
- While notes sound (playback, keyboard or MIDI audition) the screens show the selected track's instrument live, one
  marker per voice, each voice with one colour across all blocks and a note chip in the device header: sampler
  playheads, envelope dots and level bars, LFO dots, and on filters the input spectrum (grey) and output spectrum
  (block colour) with each voice's modulated cutoff. Coinciding markers stack. At most 16 voices are drawn (loudest),
  with a "+N" count beyond.
- A read-only voice probe in `unisynth` (cargo feature `probe`, not in the player build) and a watch/read API in the
  live host deliver that state to the UI with the existing ~30 ms status message.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `synth-daw`: "Slot racks" and "Slot remove and move" replaced by fixed slot blocks; "Live device graphics" extended
  with block screens and live per-voice displays. (Requirements introduced by `add-synth-daw`; this change archives
  after it.)
- `synth-live-host`: new requirement for watching one instrument and reporting its voice probe with the status.
- `unified-synth`: new requirement for a read-only voice probe behind a cargo feature that leaves the player build and
  the output unchanged.

## Impact

- `packages/daw/src/app.js`, `style.css`: `instrumentBox`, `slotRack`/`remapSlots` (compaction removed, swap kept for
  filters), new screen renderers and a live overlay drawn per animation frame; `test/editing.test.ts` 5.6a rewritten.
- `packages/daw/src/worklet.ts`, `engine.ts`: probe data added to the status message; a `watch(track)` command.
- `crates/synth-wasm`: `probe_watch`, `probe_ptr`/`probe_len` exports; host builds unisynth with `probe`.
- `crates/unisynth`: `probe` feature (voice snapshot: note, amplitude, envelope levels/stages, LFO phases, sampler
  positions, modulated filter cutoffs; filter input/output taps summed over voices). Player, size probes and goldens
  are unaffected (feature off).
- Docs: `docs/synth/daw.md`, directory `AGENTS.md` rows.

## Discipline Skills

`performance-optimization` (audio-thread budget of the taps and the per-frame overlay), `doubt-driven-review` (the
probe touches the voice render loop, cross-boundary engine → wasm → worklet), `observability-instrumentation` (the probe
is runtime instrumentation of the engine).
