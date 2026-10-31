# Design

## Context

See proposal.md (Why). Current state, measured on branch `daw-solid-spike`:

- `packages/daw/src/app.js` (`// @ts-nocheck`) is one `start(DAW)` closure. Its sections by size: device panels with
  GSM samples and the live view about 74 KB, arrangement with automation about 42 KB, transport and engine transport
  22 KB, files 12 KB, piano roll 9 KB, and audition, tracks, clips, zoom and menus about 20 KB. The song model is
  plain objects (`tracks`, `master`, from `model.ts` `fromSong`) mutated in place. Undo is `snap()` (JSON of
  tracks/master/sel/armed) and `restore()`. Every edit calls `commit()` and then `livePush()`, which exports and
  sends the song to the engine.
- The engine (`engine.ts`), worklet, bounce workers, `model.ts` and `api.ts` are already typed and stay as they are.
- The tests drive the page through ids, classes, ARIA names and `DAW.debug` / `DAW.engine`. These are the
  compatibility surface of the port.
- The spike proved the toolchain and the bridge: the Mixer in Solid (`src/ui/mixer.tsx` + `mixer-store.ts`), Vite
  for `main.js` only, hot reload with the state kept, the UI bench `bench/ui-mixer.ts`, and the contract test
  `test/mixer.test.ts`.

## Goals / Non-Goals

**Goals:**
- Every panel becomes a typed Solid component tree, and `app.js` is deleted.
- The spec budgets (specs/synth-daw) are met, and each port step is no slower than the step before it on the UI
  bench.
- The repo stays releasable after every step, with all browser tests green.

**Non-Goals:**
- No visual redesign. `style.css` selectors, layout and wording stay; a class may move to a component, but it keeps
  its name.
- No change to the engine, the song format, the server, `/api` or the static `/dsp` build.
- No component library or CSS framework. No SSR.
- No typing of `app.js` in place. It shrinks as panels move out, and new code is strict TypeScript from the start.

## Decisions

### D1. SolidJS + Vite, page bundle only (proven in the spike)
Solid compiles JSX to direct DOM operations and runs each component once. Its signals and stores update only the
nodes bound to a changed value, which suits a DAW whose display changes many single values per frame. Preact with
signals still re-renders a component when its body reads `.value`, and React reconciles each component on every
update. Vite builds only `src/main.ts` → `dist/main.js` (fixed name). The worklet and the workers stay on `Bun.build`,
because they load from fixed URLs and contain no JSX. `index.html` stays a static shell that the server rewrites, so
the token, COOP/COEP and `/dsp` are untouched. The dev server (`pnpm --filter @64k/daw dev`) proxies `/api` and the
engine files to the CLI server. Alternative: a Bun plugin running `babel-preset-solid` would need maintenance and
gives no hot reload.

### D2. One song store, reached in steps: `createMutable` while app.js mutates, `createStore` at the end
The model stays one object tree (`fromSong`), held in a Solid store in `ui/state/song.ts`, with every edit in
`ui/state/actions.ts` (wrapped in `batch`) in the end state. The doubt-driven review (task 2.1) rejected getting there
in one step. Each point below was checked against solid-js 1.9 in the browser build:
- **No big bang.** `app.js` mutates the model on 113 lines in every section, and `main` changes it daily. Under
  `createStore`, raw writes leave tracked readers stale, and writes through the proxy are silently ignored (a
  warning in dev builds only). A missed site breaks the DAW without an error.
- **Transition: `createMutable`.** `app.js`'s `tracks` and `master` become the mutable proxy (one step). All 113 sites
  keep working, and they now notify Solid. A `splice` of 1,000 notes notifies once. DOM nodes and class instances are
  not wrapped. Hot loops (canvas draw, `toSong` export, snapshots) read `unwrap(song)`, which is the raw object and
  costs O(1). `app.js` holds only proxies; mixing in raw references breaks `indexOf` and Map keys.
- **Ratchet.** Each panel port moves its writes into actions. A test counts the direct model writes left in `app.js`,
  and the count may only go down. At 0, the store swaps to `createStore`: action bodies use `setState(produce(…))`
  and components see a `DeepReadonly` type, so a direct write fails to compile.
- **View fields leave the song objects first.** `_lane`, `rollClip`, `override`, `_dev` and `_edited` are skipped by
  the snapshot. Reconcile would delete them from live objects, so they move to an id-keyed view store before
  reconcile-based undo. `clip.sel` and `note.sel` stay: they are part of the undoable state today.
- **Undo.** Reconcile per top-level key, never at the root (a root reconcile deletes `tempo`, `bars`, `meta` and the
  other keys the snapshot does not hold): `tracks` keyed by `id`, then `master`. Arrays without ids (clips, notes,
  sends, lane points) merge by position: the values are right, but identity shifts (a held clip proxy shows the next
  clip after an undone delete). So keyed lists (arrangement clips) get stable runtime ids before they are rendered
  with `<For>`, and view state holds ids, not object references. Until a panel is ported, `restore()` keeps rebuilding
  its views.
- **Snapshot cost.** `JSON.stringify` of the whole song happens per gesture (start and commit compare). This is
  measured on a 50-track song with GSM samples. If the cost is over 4 ms, samples move to a content-addressed asset
  table outside the snapshot, and commit uses a dirty flag set by actions instead of a string compare.
- **Per-frame values are never song state.** Automation playback values, meters and the playhead live in
  `ui/state/live.ts` (D3). They are not undoable, never pushed to the engine, and knobs show the live value if
  present, otherwise the song value. `toSong` and canvases read raw data outside tracking scopes; canvases redraw on a
  coarse revision signal that actions bump.
- Rejected: projection bridges for every panel (the spike's `syncMixer`), because they duplicate every field. The
  Mixer bridge stays until the store owns the Mixer's fields (task 2.6).

### D3. Display state outside the song store
Values that change per frame live in their own signals or stores in `ui/state/live.ts`: position (bars),
playing, peaks per track id, engine load, drops, automation values at the playhead, and the probe of the watched
track. The engine `onStatus` callback writes them only when the shown value changes, using the thresholds in today's
`showPos` and `meters`. Components bind them. Nothing in the song store changes at frame rate, so edits and playback
never invalidate each other.

### D4. Hot paths stay imperative inside components
The piano roll, the controller lanes, clip thumbnails and the live per-voice SVG overlays keep their current drawing
code (visible-window canvas, `drawLive`). They move into components that own the element through `ref` and redraw
from a `createEffect` that reads the relevant signals. Re-expressing the canvases as JSX would make them slower,
not faster.

### D5. Mount only what is shown
Tabs use `<Show>` / `<Switch>` on a `tab` signal, so a hidden panel is unmounted. Dialogs and menus mount on open.
Collapsed device blocks and slots render only their header. The cost: remounting a panel rebuilds it. The spike's
mixer remounts 51 strips well within one frame. The device panel is measured in its task, and if it is too slow, a
cached panel per tab (`display: none` with display signals suspended) is the fallback.

### D6. Arrangement windowing without a library
Track heights are known from the model: header height plus open automation lanes times the lane height. So the
arrangement computes row offsets from the store, renders rows whose range meets the scroll view plus one screen of
margin, and sizes a spacer to the full height. That is the same technique as today's piano roll `rollView`. Clips are
culled by bar range in each lane. The focused or selected track row is always rendered, so focus and keyboard
navigation survive scrolling (spec "Keyboard to an off-screen track"). Alternative `@tanstack/solid-virtual`: it
measures rows it can already compute, and it makes the two synchronized scrollers (headers and lanes) harder.
Adopt it only if the custom windowing becomes complex.

### D7. Event handling in components; global keys in one module
The document-level delegates in `app.js` (volume, mute/solo, arm, undo keys, transport keys, tracker keyboard) move
into the components that own the controls. The remaining global shortcuts go into `ui/keys.ts`, one `keydown`
listener with the current precedence (text fields first, then the focused area). The spike's mixer still depends on
the volume and mute/solo delegates; they move when the arrangement headers are ported.

### D8. Compatibility surface and test gates
Ids, classes, ARIA names, roles, `data-*` attributes used by tests or CSS, and `DAW.debug` / `DAW.engine` stay. Each
port step:
1. writes a contract browser test for the panel and makes it pass on the old code;
2. records the UI bench on the old code;
3. ports the panel;
4. passes all tests and is no slower on the bench (more than 10 % slower requires a justification);
5. updates `AGENTS.md` rows and lands on `main`.

The bench grows from `bench/ui-mixer.ts` into `bench/ui.ts` with named scenarios: select, knob, fader, mute,
open-song, undo, scroll, and play with each tab. It runs on a 50-track song from `project/music/released/` when
present, and otherwise on a generated 50-track song.

### D9. Directory layout
`src/ui/App.tsx` (root, mounted by `main.ts` in the final step), `src/ui/state/` (song, live, history, prefs =
`store`), `src/ui/transport/`, `src/ui/arrange/`, `src/ui/device/`, `src/ui/roll/`, `src/ui/mixer/`,
`src/ui/library/`, `src/ui/files/`, `src/ui/common/` (Knob, Picker, Menu, Dialog, Splitter). Stores live in their
own modules (hot reload keeps state; found in the spike).

## Risks / Trade-offs

- [`app.js` keeps changing on `main` during the port] → Steps are small and each lands quickly. A port step that
  conflicts with a fix replays the fix in the component, and the fix's test (if any) proves it.
- [Behavior hidden in `app.js` details (deferred mute commit, per-key undo steps, TDZ-sensitive helper order) is lost
  in the port] → The contract test per panel is written against the old code first; the spike found two such details.
  `app.js.AGENTS.md` lists known quirks and is the checklist per panel.
- [Remounting heavy panels on tab switch is slow] → Measured per panel (D5); cached fallback.
- [Windowing breaks tests that expect all tracks in the DOM] → Existing tests use the 5-track demo song, which fits
  in view. New tests cover the 50-track case.
- [Accessibility regressions from windowing (screen readers see fewer rows)] → Headers keep `aria-setsize` and
  `aria-posinset`. axe stays in the browser test.
- [Reactive proxies in hot loops] → Canvases and export read `unwrap()`. The bench catches regressions.
- [Direct writes survive in app.js after the swap to `createStore` (silently ignored)] → The ratchet test reaches 0
  before the swap, and components get a `DeepReadonly` type.
- [Bundle grows (Solid is about 7 KB)] → Irrelevant: the DAW is an editor, not part of the 64k.

## Migration Plan

1. Land the spike on `main`: Vite, Solid, Mixer, contract test, bench.
2. Song store (D2): view fields off the model, then the `createMutable` transition, the live store, per-key undo,
   the Mixer on the store, and the write-count ratchet.
3. Transport bar and playback display (D3).
4. Files, dialogs, menus, library.
5. Arrangement: headers, lanes, clips, automation lanes, windowing (D6), then track/clip commands and keys (D7).
6. Device panel: knobs, slot racks, effects, sampler/GSM, then the live view (D4).
7. Piano roll and controller lanes (D4).
8. Audition, computer keyboard, MIDI, zoom and splitters; `ui/keys.ts`.
9. Delete `app.js` / `app.d.ts`, mount `App` from `main.ts`, and update `docs/synth/daw.md`.

Every step is a separate commit series on `main`. Rollback: revert that step's commits. The previous step is
complete and tested on its own.

## Open Questions

- Whether the device panel remounts fast enough on tab switch (D5) is measured in step 6. Either outcome fits the
  plan.
- Whether `@tanstack/solid-virtual` is needed (D6) is decided during step 5. The spec does not depend on it.
