# test/editing.test.ts (sidecar: full detail of the AGENTS.md row)

Pull-only (not auto-injected).

## Contract
- Editing contract: clips, notes, tracks, slots, roll, sampler, record. One bullet per `test(...)` (verified against the file):
  - 5.4a paste trims what it covers (no overlapping clips) and undo restores
  - 5.4b note commands: select all, transpose, delete, undo — in the clip's notes
  - 5.4c linked duplicate shares one clip; a note edit reaches both placements
  - 5.4f track commands and clip loop toggle
  - 5.6a fixed slots: clear in place keeps other slots and their mod targets; filters swap with their lanes
  - 5.6c collapsed slots expand when set; a collapsed block survives a track change and is not saved
  - 5.6b knobs follow automation at the playhead; a hand edit overrides until re-enabled
  - 5.4e zoom keeps the bar under the pointer
  - no page errors
  - the piano roll draws only what is visible and hit-tests correctly when scrolled
  - resizing one of several selected notes scales every selected length by the same factor (Alt: same amount)
  - loop is off by default; the computer keyboard plays the selected track (tracker layout)
  - record: a knob turned while playing writes its lane (created), one undo step
  - Import sample encodes a GSM sample into the instrument; Remove unused drops samples no slot plays
  - active slots show screens: oscillator waveform, filter response with its cutoff mark, LFO shape
  - envelope curves redraw from their knobs; sampler settings are named controls with a draggable sample view

## Fixtures
- Shared `bigSong`/`denseSong` live in `test/fixtures.ts`. Run: `bun test packages/daw/test/editing.test.ts` (builds the page with `buildDaw({ wasm: false })`, reusing a built engine module; Chrome via `CHROME_CHANNEL`, default `chrome`; `bundled` = Playwright Chromium).
