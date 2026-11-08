# test/arrangement.test.ts (sidecar: full detail of the AGENTS.md row)

Pull-only (not auto-injected).

## Contract
- Arrangement contract (daw-solid-ts 5.1, 9.5) on the real page. One bullet per `test(...)` (verified against the file):
  - headers: click selects (header + lane marked), arm is exclusive, the A button shows and hides the lanes
  - headers: double-click the name renames (Escape cancels); arrows move focus and selection between tracks
  - clips: double-click an empty lane creates a one-bar clip there; double-click a clip opens the piano roll
  - clips: drag the title moves (snapped, one undo); drag the edge resizes; Alt+edge loops
  - clips: a marquee from an empty spot selects the clips it touches; Ctrl+D duplicates them after themselves
  - clips: with snap Off a dragged clip lands off the grid; Q aligns the selection to bars
  - automation: click adds a point, arrows edit it, Delete removes it, the last point stays; target and lanes
  - automation: handles follow the view; a focused point keeps focus after zoom + scroll; dense lanes are thinned
  - follow: while playing, the view pages when the playhead reaches the right edge; a hand scroll pauses follow
  - 50 tracks: every header and lane in order; Arrow Down walks to the last track, which scrolls into view
  - header controls drive the mixer strip (store): fader keys are one undo step each, mute is one
  - 50 tracks: fewer than 50 lanes exist and the scroll height covers all 50 (spec: Large song)
  - 50 tracks: Arrow Down past the view scrolls, and the next header exists, has focus and is selected (spec: Keyboard to an off-screen track)
  - 50 tracks: no critical or serious accessibility violations (axe)
  - no page errors

## Fixtures
- Shared `bigSong`/`denseSong` live in `test/fixtures.ts`. Run: `bun test packages/daw/test/arrangement.test.ts` (builds the page with `buildDaw({ wasm: false })`, reusing a built engine module; Chrome via `CHROME_CHANNEL`, default `chrome`; `bundled` = Playwright Chromium).
