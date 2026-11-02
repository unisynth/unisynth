# Tasks

Per port step (design D8): contract test passing on the old code → UI bench on the old code → port → all
`bun test packages/daw` green, `npx tsc -p packages/daw/tsconfig.json` clean, bench not more than 10 % slower than
before → `AGENTS.md` rows → land on `main`.

## 1. Toolchain and Mixer (spike, branch `daw-solid-spike`)

- [x] 1.1 Vite + `vite-plugin-solid` builds `src/main.ts` → `dist/main.js` (fixed name), with workers and wasm still
  on `Bun.build` in `build.ts`. Verified: `dist/` holds the same 8 files; `build-static.ts` output has the same
  layout.
- [x] 1.2 `vite.config.ts` dev server: `127.0.0.1:6465/daw/`, COOP/COEP, proxy `/api` + engine/worker files to
  `DAW_API`, `daw-dev-entry` (`order: pre`). Verified in a browser: `crossOriginIsolated`, mixer component hot
  reload keeps page state.
- [x] 1.3 `packages/daw/tsconfig.json` (Solid JSX, strict). Verified: `tsc` clean and fails on a planted error.
- [x] 1.4 `test/mixer.test.ts` contract written and green on the imperative mixer (8 tests incl. hidden tab = no DOM
  and meter-only mutations while playing).
- [x] 1.5 Mixer in Solid (`ui/mixer.tsx`, `ui/mixer-store.ts`, path updates `patchStrip`/`patchSend`). Verified: 54/54
  tests; `bench/ui-mixer.ts` on trashpanda: select 23 → 16 ms, fader 0.37 and mute 2.1 ms (equal), frames equal.
- [x] 1.6 Rebase `daw-solid-spike` on `main`, re-run the gate (tests, tsc, bench, `build-static.ts`, `/daw` from
  `serve`), add a "Dev loop" section to `docs/synth/daw.md` (two commands, ports, proxy), and merge. Verify: the
  documented commands start a working hot-reload session.

## 2. Song store, actions and history (D2, D3)

- [x] 2.1 Doubt-driven review of the store shape and the `app.js` ↔ store boundary (fresh-context reviewer; findings
  checked in the browser build of solid-js 1.9: raw writes leave tracked readers stale, proxy writes are ignored,
  root reconcile deletes keys, track reconcile deletes view fields, id-less arrays merge by position, a
  `createMutable` splice notifies once). Outcome recorded in design.md D2: strangler via `createMutable`, ratchet,
  `createStore` at the end.
- [x] 2.2 Move view-only fields off the song objects into an id-keyed view store (`ui/state/view.ts`): `_lane`,
  `rollClip`, `override` (`_dev` is a knob element property and `_edited` is undoable device state: both stay). `snap()` no longer needs a replacer for them. Verify: all browser tests,
  `DAW.debug.song()` unchanged on the demo and every `project/music` song.
- [x] 2.3 `ui/state/song.ts`: `song = createMutable(fromSong(…))`; `app.js` `tracks`/`master` point at it; `toSong`,
  snapshots and canvas draws read `unwrap(song)`. Verify: all tests; `bench/ui.ts` select/fader/knob/play and
  roll open time not more than 10 % slower than before.
  Done: 54/54. `raw()` = O(1) `$RAW` read (Solid's `unwrap` deep-walks non-proxies); `restore` batched; `tableOf`/`modeOf`
  read once and raw; clip thumbnails read raw notes. Bench (trashpanda, 3 runs, before → after): select-dev 56 → 61,
  select-mix 13.6 → 15.6, undo 380 → 465 ms; knob 6.5, fader 0.35, mute 2.1, roll ~30 ms, frames unchanged.
  **Accepted over the 10 % gate (user, temporary):** the cost is untracked reads by not-yet-ported imperative code
  through the proxy (`ui-profile.ts`: getOwnPropertyDescriptor + wrap per read). It shrinks as each panel moves to
  components with raw bulk reads; undo drops with 2.5. Baseline for later steps = the "after" numbers.
- [x] 2.4 `ui/state/live.ts`: position, playing, peaks, load, drops, automation values at the playhead, probe;
  written from `onStatus`/`applyAutomation` only on a shown change, never into the song or undo.
  Done: `pos`, `playing`, `load`, `peaks` (+ `setPeak` threshold, mixer reads it). Drops, the probe and `autoVal` stay
  where they are until a component reads them (transport 3.3, live view 6.4, knobs 6.2); `applyAutomation` already
  never wrote the song. Unit tests run on Solid's browser build via `test/solid-browser.preload.ts`. 57/57; bench =
  2.3 numbers. Unit test: an
  unchanged value does not notify; browser test: playing an automated song adds no undo step and does not change
  `DAW.debug.song()`.
- [x] 2.5 `ui/state/history.ts`: `commit`/`undo`/`redo` moved from `app.js`; restore reconciles `tracks` (key `id`) and
  `master` separately, never the root. Measure the snapshot cost on a 50-track song with GSM samples; if over 4 ms,
  add the asset table and dirty-flag commit (design D2). Unit tests: undo/redo round trip equals the saved song;
  unchanged tracks keep identity; `tempo`/`bars`/`meta` survive an undo.
  Done: 61/61 (4 history unit tests). Snapshot cost is notes, not GSM samples (max 31 KB): no asset table. Commit keeps
  its compare (drops no-op gestures); "unsaved" uses state ids (a hand-made revert to the saved values now shows
  unsaved). Found by `bench/ui-profile.ts` and fixed here: the `JSON.stringify` replacer (212 ms per knob step on
  tbl-stash-over2), uncached `getComputedStyle` in `css()` (seconds per undo), and proxy reads in lane draw /
  `laneValue`. Bench vs pre-port (trashpanda; tbl-stash-over2): select-dev 56 → 33 (83 → 53), knob 6.5 → 1.2
  (245 → 31), undo 380 → 430 (6 187 → 6 166), select-mix 13.6 → 12.5, mute 2.1 → 0.9 (71 → 46). Undo still
  rebuilds every view until the arrangement and device panels are ported (5, 6).
- [x] 2.6 Mixer reads `song` and `live` directly; delete `syncMixer`, `patchStrip`, `patchSend` and `setPeak`. Verify:
  mixer test and bench unchanged.
  Done: 62/62 (+ mixer test "edits while hidden"). `selectTrack` normalises `selected` to the store proxy (raw
  references from JSON-built tracks would write without notifying). Bench within noise (trashpanda select-mix 12.7,
  fader 0.4, mute 1.1; tbl-stash-over2 undo 6.4–6.7 s, play-mix 1–2 frames > 20 ms; one outlier run of 36 under
  machine load).
- [x] 2.7 Ratchet test (`bun test`): counts direct model writes left in `app.js` (assignments, `splice`/`push`/`sort`
  on model arrays, `Object.assign` on model objects) and fails when the count rises above the number recorded in the
  test. Verify: an added write fails the test.
  Done: `test/write-ratchet.test.ts`, baseline 102 lines; an appended `t.vol = 0` fails it (103). Strict both ways:
  a lower count asks to lower `BASELINE`, so each port records its progress.

## 3. Transport and playback display (D3)

- [x] 3.1 Contract test `test/transport.test.ts` on the old code: play/stop/second stop, position and time text, loop
  strip edit + toggle, follow hold/resume, tempo/signature edit, buffer select, load and drops readout, "edited since
  play".
  Done: 9 tests, stable over 4 runs. Found: the old bar refreshes the time readout only on the next position update
  after a tempo change (contract reads times after a seek; the port derives the time from tempo); the demo's load
  can round to 0 % and still reads > 0 after the first Stop (tails ring), so only the readout's form is checked.
- [x] 3.2 Extend the bench to `bench/ui.ts` (scenarios: select, knob, fader, mute, open-song, undo, scroll,
  play-per-tab; 50-track song from `released/` or generated). Record the baseline in this file.
  Done (`open` and `scroll` added; frame intervals now from `performance.now()` in the callback: the first rAF
  timestamp predated the bench's synchronous work by ~10 s and faked a 10 s frame). Baseline before group 3:
  trashpanda open 1 011, select-dev 33, knob 1.3, undo 444, select-mix 12.6, fader 0.4, mute 1.3, roll 34 ms;
  scroll p95 17.7 (3 > 20 ms); play-mix p95 17.7 (7), play-dev p95 17.7 (7). tbl-stash-over2 open 8 217,
  select-dev 53, knob 32, undo 6 358, select-mix 26, fader 11, mute 37, roll 6 ms; scroll p95 43.9 (20);
  play-mix p95 21.0 (46); play-dev p95 19.6 (6).
- [x] 3.3 Port the transport bar and the playheads to `ui/transport/`. Playheads move by `transform` from the
  position signal. Verify: contract, all tests, bench; spec "Playback display budget" scenario on the mixer and the
  arrangement.
  Done: `ui/state/transport.ts` + `ui/transport/TransportBar.tsx`; 76/76 (transport contract + time-follows-tempo test,
  which fails on the old bar). Bench within noise of the 3.2 baseline (tbl-stash-over2 play-dev measured 3× on both
  sides: 16–37 frames > 20 ms before, 25–35 after). Playheads keep their `transform` writes in `showPos`; they move to
  `pos` effects with the arrangement (5.2) and roll (7.2) ports, which own those elements.

## 4. Files, dialogs, menus, library

- [x] 4.1 Contract test on the old code: song list, open (dirty prompt), save success and 422 path list, export,
  import, new song, bounce dialog (render + cancel), context menus by keyboard (Shift+F10), library tree, search,
  inspect, audition, load into the track.
  Done: `test/files.test.ts`, 11 tests, stable over 3 runs (the 422 is a routed response; save round-trip, local
  validation and converted legacy entries stay in browser/library tests). Found and fixed: the focused menu item's
  shortcut (`kbd`, `--dim` on `--sel`, 4.4:1) failed axe colour contrast; it now uses `--fg`. Import of
  `<name>.song.bin` opens as `<name>.song.json`, unsaved (kept).
- [x] 4.2 `ui/common/Dialog.tsx`, `Menu.tsx` (mounted only while open, focus return to the opener), then port files and
  bounce (`ui/files/`) and library (`ui/library/`). Verify: contract, existing library/threads tests, axe.
  Done: `ui/common/{Dialog,Menu}.tsx`, `ui/state/{files,library}.ts`, `ui/files/{Files,dialogs}.tsx`,
  `ui/library/Library.tsx`; 91/91 twice (files contract 12, library unit 4). Ratchet 102 → 101 (the old tree's
  `d.open =` was a `<details>`, counted as a model write). Bench within noise (trashpanda open measured 3× each side
  in one session: 1 597–1 641 before, 1 603–1 627 after; the earlier ~1 050 baseline ran on a less loaded machine).
  Found: Solid templates with `<table><tr>` need an explicit `<tbody>`. The inspector now follows the selected track
  (target name, Audition/Load enabled) instead of a snapshot at click time; the library keeps open groups after a
  preset save. Track/lane/clip menus use `popMenu` unchanged until the arrangement port.

## 5. Arrangement (D6, D7)

- [x] 5.1 Contract test on the old code for anything existing tests do not cover: header controls (arm, automation
  toggle, rename, menu), lane double-click create, clip drag/resize/loop edge, marquee, clipboard, align, automation
  point edit/remove by keyboard, follow paging; plus a generated 50-track song.
  Done: `test/arrangement.test.ts`, 10 tests, stable over 2 runs; `bigSong()` builds the 50-track song from the demo
  (bass track × 50). Clipboard paste stays in editing.test.ts (5.4a). Found: Q (align) is a note key while the
  computer keyboard is on (the default), so plain Q aligns only with the keyboard off; Ctrl+Q (the footer's documented key) always aligns.
- [x] 5.2 Port track headers and lanes with windowing (rows in view plus one screen; focused and selected rows always
  mounted; spacer height = whole song; `aria-setsize` / `aria-posinset`). Verify: spec "Off-screen arrangement
  content" scenarios as tests, existing editing tests, axe.
  Done: `ui/state/arrange.ts` (+ unit test), `ui/arrange/Tracks.tsx`; spec scenarios "Large song" and "Keyboard to an
  off-screen track" as tests (both fail on the old code), axe on the 50-track arrangement; 109/109 twice; ratchet
  101 → 98. Deviation: no `aria-setsize`/`aria-posinset` (ARIA does not allow them on `role=group`, axe fails).
  Rows fill (clips, automation lanes) only while in the view window; a row pinned only by selection/focus is empty
  until scrolled to; rows entering on scroll fill from a 6 ms/frame queue. The automation target `<select>` builds
  its ~270 options on first use (they were most of the scroll cost). Found: the initial selection was never
  published (no header marked until a click); fixed. Store writes notify at once: a recorded lane is pushed with
  its first point. Bench before → after (same session): trashpanda open 1 582 → 1 151, select-dev 32 → 32,
  undo 436 → 63, select-mix 14 → 14, scroll p95 17.4 → 18.2 (1 → 1 > 20 ms); tbl-stash-over2 open 8 836 → 1 358,
  select-dev 57 → 37, undo 6 148 → 226, select-mix 24 → 15, scroll p95 38.8 → 25.5 (20 → 8).
- [x] 5.3 Port clips, clip thumbnails (canvas, D4) and automation lanes with point editing. Verify: contract + editing
  tests, bench scroll and select.
  Done: `ui/arrange/Clip.tsx` (reactive clip; canvas thumbnail from `thumb.ts`, unit-tested against the old per-note
  layout; screenshot matches) culled per lane to `barWindow` (view-width pages ± 1); `renderLane` = lane version bump.
  Automation lanes: `addLane` returns its header/lane elements, the row maps `t.lanes` to them (reactive list, no
  rebuild on add/remove; SVG + point editor stay imperative, D4). Found and fixed on the way: `Engine.start` had no
  in-flight guard (each early key press made an AudioContext; orphan statuses blanked the live displays: the
  long-standing flaky live-chord test, now 6/6), and dialogs were removed only on the async close event (a query
  right after Cancel still found them). 115/115 twice. Bench (5.2 → 5.3): trashpanda undo 63 → 56, select-mix
  14 → 12, scroll p95 18.2 → 18.6 (1 → 2 > 20 ms); tbl-stash-over2 undo 226 → 217, scroll p95 25.5 → 23.1
  (8 → 9), play frames unchanged.
- [x] 5.4 Move the volume, mute/solo and arm delegates into the header and strip components (D7). Track and clip
  commands go through actions. Verify: mixer and header stay in sync (mixer test), undo steps unchanged.
  Done: no document-level delegates left for mute/solo, arm, volume or the headers; header and strip controls call
  `TrackActions` / `MixerActions` (`toggleMuteSolo`, `toggleArm`, `setVolume`, `headerClick`/`headerKey`/`headerMenu`).
  New contract test: header fader keys and mute drive the mixer strip, one undo step per key and per mute. Clip
  pointer gestures stay as the lanes-container listeners in app.js (they already call `arrCmd`/`place`); global keys
  move to `ui/keys.ts` in group 8. 117/117 twice; ratchet 98; bench within noise (fader 0.12 → 0.04 ms: no
  `querySelectorAll` sync of the other fader).

## 6. Device panel and live view (D4, D5)

- [x] 6.1 Contract test on the old code for anything existing tests do not cover: knob keyboard/drag/wheel and value
  text, effect add/reorder/bypass/remove/mode, preset save/load, block collapse persistence, sampler settings, mod
  matrix rows.
  Done: `test/device.test.ts` (6 tests, 3/3 runs on the old code). Already covered elsewhere: knob keys + undo
  (browser), block collapse, slot cards, screens, sampler settings + waveform drag, GSM import (editing). Found on
  the old code, for 6.2: knobs have no wheel handler (not added: not asked); focus falls to `<body>` after a
  successful preset save (panel rebuild); the mod-matrix amount knob has an empty `aria-label`.
- [x] 6.2 `ui/common/Knob.tsx` bound to `devices[i].values[name]` (automation writes update only that knob), then slot
  racks, mod matrix, screens (osc/filter/LFO/envelope) and effect boxes. Verify: contract, editing tests (5.6a/c
  slot tests), bench knob ≤ 1 frame.
  Done: `ui/common/Knob.tsx`, `ui/device/Device.tsx`, `ui/state/device.ts`. Automation writes a per-device store
  (only on change), so playback mutates just the automated knobs and the screens drawn from them (new test).
  Screens keep their `_draw` (D4), redrawn by an effect from the store values they read. The sampler block stays an
  island rebuilt on `renderDevice()` until 6.3. The panel mounts only while the Device tab shows (D5, done here: on
  the mixer the hidden panel rebuilt on every track select). Fixes on the way: effect names follow their mode
  (`devName` read the mode raw); the menu's outside-click listener raced Chrome's input-before-timers priority
  (attached in a `setTimeout`) and left menus open; focus stays on Save preset after saving; the mod amount knob
  is named. 124/124 twice, ratchet 98. Bench (5.4 → 6.2), trashpanda / tbl-stash-over2: undo 55 → 15 / 217 → 182 ms,
  select-mix 13.2 → 0.7 / 16.4 → 1.4 ms, select-dev 32 / 32 ms (unchanged), knob 1.2 / 30.6 ms (over2: the two
  4 MB undo snapshots per step, as before the port; not a panel cost), playback p95 ≤ 18.5 ms. Screenshots of the
  Bass and Lead panels match the old build.
- [x] 6.3 Port GSM/sampler editing (import WAV, choose/play/remove unused, waveform drag). Verify: editing tests for
  samples.
  Done: `ui/device/Sampler.tsx` (source, description, GSM buttons, settings, canvas waveform drawn by an effect).
  New contract test in device.test.ts (passed on the old code first). The device panel has no imperative islands
  left (`devRev` gone). One deliberate change: a drag that turns the sample's own loop into a custom one is now
  inside the drag's undo step (the old code snapshotted after that conversion, so undo kept it). Screenshot of a
  Pad sampler slot matches the old build. 125/125, ratchet 98 → 96, bench unchanged.
- [x] 6.4 Live per-voice view as a component owning its SVG (`drawLive` kept, D4), active only while the Device tab
  shows and Live is on. Verify: browser live-display tests, `DAW.debug.live()` cost not higher than baseline.
  Done: `ui/device/live.ts` owns probe parsing, spectra and the overlay drawing (SVG strings, D4); screens register
  while mounted instead of being queried each frame; the voice chips are a component (`chips` signal); the frame
  loop runs only while the Device tab shows (the watch was already off otherwise). `DAW.debug.live()` with a 6-note
  chord on the Lead, 2 runs each, old → new: probe 0.23 → 0.23 ms, draw 0.25 → 0.25 ms. New test: the hidden Device
  tab holds no content and comes back with edits made meanwhile. 126/126 twice.
- [x] 6.5 Measure the Device tab remount on a 50-track song with a full instrument. If over one frame, keep a cached
  panel with display effects suspended (D5 fallback). Record the result in design.md.
  Measured (bench `tab-dev`: Mixer → Device with the fullest instrument selected; handler ms / ms to the frame after):
  trashpanda (Lead (Falcon) 1, 50 values) 17.2 / 54 ms, tbl-stash-over2 (29 values) 18.9 / 84 ms, after: collapsed
  slots and blocks render only their header (D5 text), `ip()` indexed by name, and the production build fix below.
  Always-mounted experiment (the D5 fallback without suspension): 1.3 / 34 ms and 1.3 / 55 ms, but select-mix
  0.7 → 19 ms and 1.3 → 25 ms (the hidden panel rebuilds on every selection). Neither stays within one frame;
  decision (user): keep the remount, spec unchanged; recorded in design.md D5.
  Found: `vite build` shipped solid-js's development runtime (Vite's `development|production` condition followed
  NODE_ENV: unset in Bun scripts, "test" under bun test). vite.config.ts now pins production for `build`. All
  earlier bench numbers of this change measured the Solid parts on the dev runtime (the imperative baselines were
  unaffected); select-dev 29 → 26 ms, tab-dev handler 20 → 17 ms with the production runtime.

## 7. Piano roll and controller lanes (D4)

- [x] 7.1 Contract test on the old code for anything existing tests do not cover: note draw/select/move/resize,
  velocity paint, quantize/legato, zoom anchor, scroll-to-notes, 118-bar 950-note clip opens fast.
  Done: `test/roll.test.ts` (7 tests, 2/2 runs on the old code). The long clip opens in 23-37 ms (last pointerdown
  to two frames after the roll shows), well under the 0.3 s noted in the design.
- [x] 7.2 Port the roll, keys and CC lanes as components owning their canvases (visible window, `unwrap` reads).
  Verify: contract, editing and controllers tests, open time of the large clip ≤ the baseline (about 0.3 s).
  Done: `ui/roll/Roll.tsx` renders the roll panel only while its tab shows (the hidden panel held 73 keys, two
  canvases and the toolbar); the canvases stay imperative (D4): app.js binds its existing handlers to each mount
  (`onRollMount` wraps them in place) and keeps the grid's scroll position across remounts. New contract scenario:
  hidden roll empty, an undo made meanwhile shows on return. Long clip opens in 24-41 ms (7.1: 23-37 ms); roll tab
  ~30 ms and boot (6 alternating runs, median 1302 vs 1312 ms) unchanged against the 7.1 build. 134/134.

## 8. Audition, keyboard, MIDI, zoom, splitters

- [x] 8.1 `ui/keys.ts`: a single global `keydown` with the current precedence (text fields, focused area, transport,
  tracker keyboard by `e.code`, undo/redo). Verify: existing keyboard tests plus a test of each shortcut in
  `docs/synth/daw.md`.
  Done: `test/keys.test.ts` (7 tests, written and passing on the old code) covers each global row of the key table
  and the precedence; editor-local rows stay covered by the arrangement, roll and files tests. `ui/keys.ts` replaces
  the four window/document listeners of app.js (one table, capture + bubble phases, see D7). 141/141 twice.
- [x] 8.2 Port audition (armed track, MW/PB sliders, Web MIDI), zoom (anchored) and splitters (persisted sizes).
  Verify: controllers test, editing zoom test.
  Done: contract `test/controls.test.ts` (5 tests on the old code: armed/⌨, Web MIDI stub, anchored arrangement
  wheel zoom, splitters incl. reload). `ui/footer/Audition.tsx` + `state/audition.ts`, `ui/common/Splitter.tsx` +
  `state/layout.ts` (drops the MutationObserver that re-read computed style on every zoom), `state/persist.ts`
  shared, `ui/zoom.ts` (`anchorScroll`, unit-tested). Footer and splitter geometry identical to the old build.
  149/149 twice.

## 9. Finish

- [ ] 9.1 Delete `app.js`, `app.d.ts`; `main.ts` mounts `ui/App.tsx`. Remove `// @ts-nocheck` users. Verify: tsc
  clean, `rg "app.js" packages/daw/src` empty, all tests green.
  Plan (one commit each, tsc + suite twice + bench): 1 model helpers, 2 core (undo, say, selection), 3 engine link,
  4 transport, 5 arrangement, 6 piano roll, 7 device helpers, 8 files/tracks, 9 `ui/App.tsx` + delete. The write
  ratchet counts app.js plus the split modules (`SPLIT`).
  - [x] step 1: `ui/model/params.ts`, `track.ts`, `bytes.ts` (typed, unit-tested on the real engine tables);
    `SLOTS` moves there (Device.tsx re-exports); dead law table `L` removed. 156/156 twice, bench unchanged.
  - [x] step 2: `ui/app/session.ts` (engine, api, song store, selection as live bindings + setters) and
    `ui/app/core.ts` (say, history, snap/commit/undo/redo/restore with hooks). Shared app.js state moves the same way
    in later steps: an exported `let` plus a setter, so reads stay unchanged. 158/158 twice, bench unchanged.
  - [x] steps 3–4 (together: the clock reads the transport state every frame): `ui/app/transport.ts` (length,
    playhead, loop, play, follow hold; position maths reuse the transport store's) and `ui/app/engine.ts` (push,
    play/stop/seek, frame clock, meters, load, drops, buffer, open filters; hooks into the arrangement). Ruler and loop
    strip DOM stay for step 5. 161/161 twice, bench unchanged on both songs.
  - [x] step 5a: `ui/app/automation.ts` (lane values, record, lane rows), `ui/app/arrange.ts` (`BW`),
    `ui/app/theme.ts` (`css`); ratchet treats the drag alias `dg` like `drag`. 161/161 twice, bench unchanged.
  - [x] step 5b: `ui/app/clips.ts` (clip commands, clipboard, lane gestures, keys, menus; typed gesture union),
    snapping into transport.ts, `focusArea` into arrange.ts. 161/161 twice, bench unchanged.
  - [x] step 5c: arrangement view into `ui/app/arrange.ts` (ruler, loop strip, playheads, follow, relayout, zoom;
    hooks for the roll). DOM order of #lanes/#ruler and a screenshot match the previous build; roll-open bench
    alternating against it: 27–34 vs 30–37 ms (noise). 161/161 twice.
  - [x] step 6: piano roll into `ui/app/roll.ts` (typed gesture unions; audition keys via `initRoll` hooks;
    `rollClips` typed). 161/161 twice, bench unchanged (roll 29.8 / 7.9 ms).
- [ ] 9.2 Integration check of the spec budgets on the 50-track song: bench every scenario against the step-3
  baseline, "Edit response budget" and "Playback display budget" as browser tests. Verify: `/daw/` from `serve`,
  `/dsp` static build loads and bounces, hot-reload dev loop works.
- [ ] 9.3 `docs/synth/daw.md` UI structure (components, stores, display state), `packages/daw/AGENTS.md` (remove the
  `app.js` sidecar, add `src/ui/**` directory `AGENTS.md`), and memory of the conventions.

## Workflow follow-up

- Archive the change once step 9 is merged and the user has reviewed the DAW on a large song.
