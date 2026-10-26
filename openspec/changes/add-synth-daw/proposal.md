# Proposal

## Why

The unified synth (`crates/unisynth`, change `add-unified-synth`) has no way to make music with it. Songs are
`synth-mix` blobs written by converters or tests; there is no place to write notes, shape a patch while hearing it, or
mix tracks. The usual demoscene route (VST plugin in Ableton/Renoise plus a project converter) needs a converter per
DAW and keeps the music outside the intro project. `unisynth` and `synth-core` already build for `wasm32`, so a
browser DAW can play the **same code the intro plays** and keep the song in the repo as editable JSON, next to
`intro.json`. Research and options: `research/daw-research.md` (option B).

## What Changes

- **Song source format `song/1`** (`project/music/*.song.json`, schema in `schema/`): musical time (960 ticks per
  beat, one tempo, time signature for display), tracks, note clips placed on tracks (a clip can be placed many times:
  linked instances), devices per track (kind + normalized values), track volume, receives (main / sidechain),
  per-track automation lanes, names and colours. It is the editable source; the `synth-mix` blob is its export.
- **Patch presets `us-patch/1`** (`project/music/patches/*.patch.json`): one device (kind + values keyed by parameter
  name), so patches can be shared between songs and diffed in git.
- New std crate **`crates/song`**: serde model, validation with JSON-path errors, `export` (song → `synth-mix` blob:
  ticks → samples, clips flattened into one lane per track, automation quantized to the blob's bytes), `import`
  (blob → song: devices, chunks, tracks, routing and automation exact; each lane becomes one clip), preset read/write.
  Binary `song` with `export`, `import`, `render` (WAV through `synth_core::mix::Player`) and `info`.
- New std cdylib **`crates/synth-wasm`** (plain C ABI like `crates/wasm`): a **live host** that runs `unisynth` devices
  from an editable event schedule with the same rules as `Player` (100-frame sub-blocks, automation at the block start,
  sample-exact notes), takes edits while playing (schedule swap, `set_param`, device rebuild), plays audition notes,
  reports per-track peak levels, and exports the device parameter tables (names, groups, laws, units) for the UI.
  It also wraps `song` export/import so the browser and the CLI share one implementation.
- New package **`packages/daw`**: a browser DAW served by `packages/cli` at `/daw`.
  - Transport: play/stop, loop range, position (bars.beats.ticks and time), tempo, follow playhead, CPU load.
  - Arrangement view: track headers (mute, solo, arm), clips on a bar grid, drag/resize/duplicate/link, automation
    lanes.
  - Piano roll for the selected clip: draw/move/resize notes, velocity lane, snap, keyboard shortcuts.
  - Device panel: the instrument and effect editors **generated** from the exported parameter tables (physical units,
    grouped by slot), preset load/save.
  - Mixer: track volume, inserts, post-fader sends, bus tracks, peak meters, master effects.
  - Track selection drives the device panel, piano roll and mixer highlight.
  - Instrument library: one searchable list over project presets, the GM bank, Tunefish programs, WaveSabre song
    instruments and repo test patches (inventory: `research/synth-preset-inventory.md`), with load-into-track
    through `synth-convert`, and a GM sampler editor with a key-region map.
  - Audition from the computer keyboard and Web MIDI; undo/redo; save (Ctrl+S) through a confined `/api/music` route.
  - Engine: `synth-wasm` in an AudioWorklet at the song's sample rate; edits over the worklet port. Offline bounce in
    a Worker renders the **exported blob** through `Player` (the exact intro sound) to WAV.
- `packages/cli` server: serves the DAW, read-only `/api/library` and `/api/bank` (GM bank from `GM_DLS` or the OS path), and `/api/music` (list/read/write confined to `project/music/`, same token and
  Origin rules as `/api/project`).

Assumptions (user can change them in review): arrangement view only (no Session View); piano roll first, a tracker
pattern view is a later change; a separate page rather than a panel inside the intro editor; work starts now against
instrument kind 64, and effect editors appear as kinds 65–73 land, because the UI is generated from the tables.

Out of scope: tempo changes over time, audio clips and recording, plugin hosting, Session View, tracker view, clip
envelopes, a pattern-compressed blob format, playing legacy kinds 0–39 (convert them with `synth-convert` first),
intro-timeline sync to beats, a WebSocket control API for the DAW.

## Discipline Skills

`doubt-driven-review` (the `song/1` and `us-patch/1` formats and the live host's equivalence with `Player` are
contracts that are hard to change later), `security-hardening` (the import path and `/api/music` read untrusted bytes
and paths), `performance-optimization` (the live host must fit the 128-frame AudioWorklet budget), `systematic-debugging`
(live vs offline render mismatches), `code-simplification`.

## Capabilities

### New Capabilities
- `song-source`: the `song/1` song source and `us-patch/1` preset formats, validation, export to and import from the
  `synth-mix` blob, and the `song` CLI.
- `synth-live-host`: the wasm live engine: schedule, edits while playing, audition, meters, parameter tables, and its
  equivalence with `synth_core::mix::Player`.
- `synth-daw`: the browser DAW: transport, arrangement, piano roll, generated device editors, mixer, presets,
  audition, undo, save, offline bounce, and its file API.

### Modified Capabilities
- (none) `synth-mix` behaviour does not change; the live host reuses its track render step (design D3).
  `editor-control-api` is unchanged; `/api/music` is a new route of the same server, specified under `synth-daw`.

## Impact

- New crates `crates/song` (std, in `default-members`) and `crates/synth-wasm` (std cdylib, wasm32 only, not in
  `default-members`). `crates/synth-core`: the per-sub-block track step moves out of `Player` into a shared function
  with an event/automation source trait (no behaviour change, golden hashes must stay equal).
- New package `packages/daw`; `packages/cli/src/server.ts` gains `/daw` and `/api/music`.
- New `project/music/` directory, `schema/song.schema.json`, `schema/us-patch.schema.json`.
- Depends on `add-unified-synth` (kind 64 now; kinds 65–73 as they land; `synth-convert` for library imports) and on
  its task group 6a, added for this DAW:
  - **`ctl.wheel`, `ctl.bend` and `ctl.bend-range` instrument parameters** (indices 180–182, automatable; 177–179 hold the sampler's sample indices). The wheel exists only as live input
    (`UInstrument::mod_wheel`, mod source 7), bend has a range parameter but no input, and the blob carries only notes
    and parameter automation ("songs never carry bend").
  - **A DLS-instrument sampler source** (region per note, packed bank/program index). Until 6a.2 lands the DAW
    offers "GM.DLS wave" and marks the instrument source as pending.
- `add-audio-to-synth` can write `song/1` instead of raw blobs once both exist (noted, not changed here).
- Docs: `docs/synth/daw.md` (new), `docs/architecture.md` audio section, `crates/AGENTS.md`, `packages/AGENTS.md`,
  per-crate/package `AGENTS.md`.
- Mockup: `openspec/changes/add-synth-daw/mockup/index.html`.
