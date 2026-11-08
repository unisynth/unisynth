# test/device.test.ts (sidecar: full detail of the AGENTS.md row)

Pull-only (not auto-injected).

## Contract
- Device panel contract (daw-solid-ts 6.1-6.3, D5). One bullet per `test(...)` (verified against the file):
  - knob: drag up (160 px = full range, Shift 5× finer) is one undo step; text follows; double-click resets
  - effects: add (with a mode), mode change keeps only the mode, reorder moves lanes, bypass, remove drops its lanes
  - instrument bypass and preset save: written to project/music/patches, listed, loadable on another track
  - preset save: Escape cancels (nothing written), focus back on the button
  - mod matrix: a row lights up with its source kind; target and amount are one undo step each
  - sampler block: description, Play, Choose, source select (one undo step), Remove unused count
  - playback: automation changes only the automated knob and the screen drawn from it
  - hidden Device tab holds no content; it comes back on show with edits made meanwhile (D5)
  - no page errors

## Fixtures
- Shared `bigSong`/`denseSong` live in `test/fixtures.ts`. Run: `bun test packages/daw/test/device.test.ts` (builds the page with `buildDaw({ wasm: false })`, reusing a built engine module; Chrome via `CHROME_CHANNEL`, default `chrome`; `bundled` = Playwright Chromium).
