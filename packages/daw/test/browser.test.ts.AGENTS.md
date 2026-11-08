# test/browser.test.ts (sidecar: full detail of the AGENTS.md row)

Pull-only (not auto-injected).

## Contract
- Headless Chrome against the real server on a temp `project/music` (demo + patches). One bullet per `test(...)` (verified against the file):
  - a drawn note is heard
  - a knob turned from the keyboard changes the output and undo restores it exactly
  - save writes the same song back, and the bounce equals `song render`
  - an invalid edit cannot be saved and points at its JSON path
  - plays through the AudioWorklet: position advances and meters move
  - the computer keyboard auditions the armed track while stopped (5.8)
  - a watched track's chord comes back in the status as one probe record per voice
  - live view off: no watch, no probe, no overlays; the choice survives a reload
  - concurrent engine starts (several keys pressed before the first start finished) make one engine
  - live displays: a chord shows one chip and one marker per voice; silence clears them
  - no critical or serious accessibility violations (axe)
  - a song whose first track automates an envelope time opens (lane labels format before the helpers load)
  - no page errors
  - seeks while playing keep playing; a dead engine is reported, reads 0% and Play restarts it
  - audio drops (output underruns while playing) show next to the engine meter with where they happened
  - Run open filters: a remembered listening switch that reaches the live engine and survives an engine restart

## Fixtures
- Shared `bigSong`/`denseSong` live in `test/fixtures.ts`. Run: `bun test packages/daw/test/browser.test.ts` (builds the page with `buildDaw({ wasm: false })`, reusing a built engine module; Chrome via `CHROME_CHANNEL`, default `chrome`; `bundled` = Playwright Chromium).
