# Proposal

## Why

The DAW UI is one 205 KB, 2,320-line untyped `packages/daw/src/app.js` closure: about 120 event listeners,
107 `querySelector`, 37 `innerHTML` templates and 36 top-level mutable `let`s. Edits rebuild whole panels
(`renderAll`, `rebuildTracks` and `renderDevice` from about 40 call sites), so cost grows with song size: selecting a
track on a 51-track song takes 23 ms. Every change risks breaking a distant view; `app.js` had 7 commits in the last 5
days. A spike (branch `daw-solid-spike`) ported the Mixer to SolidJS with Vite. All 54 tests pass, selecting a track
drops to 16 ms, a hidden mixer holds no DOM, and components hot reload while the song keeps playing.

## What Changes

- Port the DAW UI from `app.js` to typed SolidJS components (`.tsx`), one panel per step, keeping every user-visible
  behavior, accessible name, keyboard shortcut and the `DAW.debug` test hooks.
- Move the song model, selection, transport and undo into typed Solid stores. Edits become targeted store updates
  instead of panel rebuilds.
- Render only what is shown: hidden tabs and dialogs are unmounted; the arrangement renders only the visible tracks and
  bars; the piano roll and controller lanes keep drawing only the visible window on canvas.
- Per-frame displays (playhead, position, meters, engine load, automated knobs, live per-voice view) update only the
  nodes whose values change, outside component re-rendering.
- Build the page with Vite + `vite-plugin-solid`. Keep `dist/` file names, the worklet and worker builds (Bun.build),
  the CLI server, the token injection and the static `/dsp` build unchanged. Add a `vite` dev server with hot reload
  that proxies `/api` and the engine files to the CLI server.
- Type-check the DAW package with `tsc` (strict). `app.js` and `app.d.ts` are removed when the last panel is ported.
- Add a UI bench (`bench/ui-*.ts`) and a contract browser test per panel before it is ported.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `synth-daw`: adds requirements that hidden and off-screen content is not rendered, and that playback displays and
  single edits stay within frame and latency budgets on large songs. Existing requirements are unchanged and remain
  the acceptance contract of the port.

## Impact

- Code: `packages/daw/src/app.js` (removed in the final step), new `packages/daw/src/ui/**` (components, stores),
  `packages/daw/src/main.ts` (mounts the root component), `packages/daw/build.ts`, new `vite.config.ts` and
  `tsconfig.json`; `packages/daw/src/style.css` keeps its selectors.
- Dependencies (pnpm, `@64k/daw` only): `solid-js`; dev: `vite`, `vite-plugin-solid`, later
  `@tanstack/solid-virtual` if the arrangement needs it. Nothing reaches the 64k player.
- Tests: existing `packages/daw/test/*.test.ts` stay unchanged as the regression gate. A new contract test is added
  per panel (the mixer test exists).
- Docs: `docs/synth/daw.md` (UI structure, dev loop), `packages/daw/AGENTS.md`.
- Parallel work: `unify-v2-algorithms` touches 4 lines of `app.js`, and DAW fixes land often. Each step ports one
  panel and lands on `main` quickly to keep conflicts small.

## Discipline Skills

`performance-optimization` (frame and edit budgets, bench before and after each panel), `doubt-driven-review`
(store shape and the app.js/Solid boundary before panels depend on it), `systematic-debugging` (behavior differences
found by the contract tests), `code-simplification` (after each port, remove the bridge code it no longer needs).
