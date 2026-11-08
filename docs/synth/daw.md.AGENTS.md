# daw.md (sidecar: full detail of the AGENTS.md row)

Pull-only (not auto-injected).

## Sections
- Pieces: `song/2` source (`crates/song`, `schema/song.schema.json`), `us-patch/2` presets, `song` CLI (`export`, `import`, `render`, `info`, `check`), shared `Mixer` step, live host `crates/synth-wasm`, browser DAW `packages/daw`, server routes `packages/cli/src/music.ts`.
- Formats: `song/2` example, notes/points, export rules (tick → sample, flattening, muted/bypassed), import + pattern cutting (`crates/song/src/patterns.rs`, add-import-patterns), `--meta` config, `about` descriptions, validation issues by JSON pointer. `song/1` files migrate once on read (`song migrate`).
- Live playback vs bounce table; load figures (task 3.7); engine load readout and CPU; GSM samples (import, 30 s cap); device panel + live view (`probe`); mod wheel / pitch bend + converted library entries (`synth-convert --example library` → `project/music/.library.json`).
- UI structure (SolidJS + TS, layer rules, undo snapshots, windowing, budgets); using the DAW (`bun packages/daw/build.ts`, `bun packages/cli/src/cli.ts serve`, `?song=`, `?tab=`, dev loop, static preview); keys table.
- Trailing "Not yet" list is stale in the doc (names ctl.wheel/bend 6a, library conversion, DLS-instrument source as pending).

## History
See change: add-synth-daw, daw-solid-ts, add-device-live-view, add-import-patterns, add-synth-threads, add-wasm-threads.
