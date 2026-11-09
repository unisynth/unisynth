# packages/daw testing (sidecar: overflow of the AGENTS.md intro)

Pull-only (not auto-injected).

## Run
- `bun test` from the repo root (root `bunfig.toml` preloads `packages/daw/test/solid-browser.preload.ts`); one file: `bun test packages/daw/test/<name>.test.ts`. Unit tests sit next to their modules (`src/model.test.ts`, `src/ui/**/*.test.ts`).
- Browser tests start the real server (`packages/cli`) on a temp `project/music`, build the page with `buildDaw({ wasm: false })` (reuses a built engine module) and drive Chrome through Playwright. `CHROME_CHANNEL` (default `chrome`; `bundled` = Playwright Chromium).
- `wasm-native.test.ts`: engage case needs `GM_DLS` (path of a GM bank), skipped without; ~2 min.
- Benches (`bench/*.ts`) are run by hand: `bun packages/daw/bench/<name>.ts` with the env vars in their rows.
- Type-check: `npx tsc -p packages/daw/tsconfig.json`.

## Test cases (names verbatim from the files)

### test/budgets.test.ts
- select a track from its mixer strip: selection and strips within one frame
- fader step: one frame; its dB readout and the header fader follow, no other strip changes
- mute and solo: one frame each, only that strip changes
- send level step: one frame, only the sending strip changes
- knob step in the device panel: one frame, only the knob, its filter screen and the preset's edited mark change
- select a track with the Device tab shown: one frame, the panel shows the selected track's chain
- playback with the mixer shown: ≥ 95 % of frames under 20 ms, only meters change, no strip rebuilt
- playback with the device panel shown and the live view on: ≥ 95 % of frames under 20 ms, no knob rebuilt
- dense automation (≈ 6 MB of lane points, like tbl-stash-over2): knob step and mute each within one frame
- dense automation: undo of one step within one frame; only the restored track's lanes change

### test/controllers.test.ts
- a pitch-bend point is saved, heard in the bounce, and the bounce equals `song render`
- the audition sliders set the armed instrument's ctl.wheel and ctl.bend live; PB springs back

### test/controls.test.ts
- the armed track follows selection; ⌨ toggles the computer keyboard and says so
- Web MIDI: connect names the inputs and the armed track; notes, CC1 and pitch bend reach the audition
- Ctrl+wheel zooms the arrangement at the pointer: the bar under it stays
- splitters: arrows (Shift = coarser) and drag resize within limits; sizes survive a reload
- no page errors

### test/files.test.ts
- song list: title, length and composer per song (file name when there is no author), the current song marked with its track count; an edit adds the unsaved dot (rows found by `data-file`)
- song search filters by title, composer and file name; Escape clears it
- switching with unsaved changes asks: Cancel stays, Don't save opens the other song
- save: clean afterwards; a 422 from the server lists the JSON paths and a path button selects its track
- export: summary dialog, Write blob writes <song>.song.bin; Close returns focus to Export
- import: a blob opens as a new unsaved song; a bad file says why
- new song: asks to save the unsaved import, then one instrument track as untitled-1
- bounce: Render the loop gives a named WAV link; Cancel stops a running render
- track menu by keyboard: Shift+F10 opens it on the first item, arrows skip disabled items, Escape returns focus
- library: sources with counts, search filters and opens groups, inspect, audition, load a preset (one undo)
- no critical or serious accessibility violations with a dialog or a menu open (axe)
- no page errors
- phone width: one column, the browser is a drawer (Songs button, Escape, scrim, picking a song closes it); desktop unchanged

### test/keys.test.ts
- Space plays and pauses; Home goes to the song start; never from a text field
- L toggles the loop, Ctrl+L loops the selected clip, F follow, Shift+R record; letters in a field type
- Ctrl+Z / Ctrl+Shift+Z / Ctrl+Y undo and redo from any control, not inside a text field
- + / − zoom and Shift+Z fits the area with focus: arrangement, then piano roll
- computer keyboard: note keys win over letter shortcuts; [ ] octave, ; ' velocity, ` on/off
- Ctrl+S saves from anywhere, a text field included
- no page errors

### test/library.test.ts
- a converted Tunefish program loads as instrument + insert effects; an unconverted one is disabled

### test/mixer.test.ts
- one strip per track, the master last, + Bus before it; inserts and sends listed
- fader (keyboard) changes the volume, its dB readout and the header fader; one undo per key step
- mute and solo toggle aria-pressed and the track; undo restores
- add a send (no loop targets offered), change its level, remove it; undo brings it back
- add an insert, bypass it, the strip follows; undo removes it
- clicking a strip name selects the track
- a hidden mixer has no DOM; showing it again rebuilds the strips
- edits while the mixer is hidden show when it opens (header mute, then undo of a fader step)
- meters follow the engine while playing (meter attributes only, no strip rebuilt) and fall back after stop

### test/playback-state.test.ts
- playing an automated song changes no saved value and adds no undo step; the automated knob follows

### test/roll.test.ts
- draw, select, move, delete: double-click adds a note on the grid, double-click on it deletes; one undo step each
- velocity lane: drag one stem, sweep-paint from an empty spot; one undo step each
- quantize (100% and 50%) and legato, on the selection or every note
- opening a clip scrolls its notes into view
- Ctrl+wheel zoom keeps the step under the pointer
- a 118-bar clip with 950 notes opens in the roll fast and draws only the visible part
- hidden roll holds no content; shown again it reflects an undo made meanwhile (D5)
- no page errors

### test/threads.test.ts
- isolated: threaded bounce == single-thread bounce == song render (whole song, dense)
- isolated: loop range threaded == single-thread
- not isolated: the DAW loads and bounces single-threaded without errors
- a helper that fails to start: the bounce falls back and still equals song render
- cancel terminates every worker of the bounce; a new bounce works

### test/transport.test.ts
- initial state: loop off, follow on, record off, position at the start, length = song bars
- seek on the ruler; the time follows the tempo field
- signature sets the beat grid and the beats of the position
- loop field: valid range turns the loop on, invalid input is restored; ⟲ toggles
- length field: grows the song, never below the last clip
- follow and record toggle (buttons and F / Shift+R)
- play: position and time move, the load readout shows; an edit shows 'edited since play'; stop twice returns to the start
- buffer and open-filter settings are remembered
- header Undo / Redo buttons undo and redo the last edit

### test/wasm-native.test.ts
- demo: the wasm bounce equals the native render bit for bit

### test/write-ratchet.test.ts
- direct model writes in the session modules only go down
- the counter sees a direct write and ignores DOM writes
