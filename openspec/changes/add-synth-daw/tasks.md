# Tasks

## 1. Mixer refactor in synth-core (D3)

- [x] 1.1 Record current golden hashes (`synth-core` player tests, `wavesabre` and `tfsynth` goldens) as the baseline
- [x] 1.2 Extract `mix::Mixer` + `Source` trait from `Player::render_sub`; `Player` = `Mixer` + blob source; doubt-driven review of the split
- [x] 1.3 Verify: all synth crates' tests and goldens byte-equal (56 lab songs hashed before/after); `cargo check` i686 `no_std` and wasm32 still pass; size probe measured: +192 B `.text` (xz +120 `all` / +196 `slaughter`); user accepted, size pass deferred to 1.4
- [ ] 1.4 Size pass on the shared `Mixer` before compo builds (goal: `ws-size-probe` back within noise of the pre-split 25 396 B `.text`)

## 2. Song source crate `crates/song` (song-source)

- [x] 2.1 Write `schema/song.schema.json` and `schema/us-patch.schema.json`; serde model with `deny_unknown_fields`
- [x] 2.2 Tests first: round trip, linked placements, tick→sample, same-sample ordering, overlapping same-pitch notes, validation paths (unknown kind/param/id, cycle, ranges, non-finite)
- [x] 2.3 Parameter tables per kind from `unisynth` host data (name, group, law, unit, default, options, structural); names frozen with chunk v1
- [x] 2.4 Export to the `synth-mix` blob (deterministic) and validation with JSON paths
- [x] 2.5 Import from the blob (exact devices/routing/automation; lanes → one clip; legacy kinds → `synth-convert` error); fuzz the import path with truncated/hostile blobs (security-hardening)
- [ ] 2.5a Clip controller envelopes: flatten wheel/bend per placement into `ctl.wheel`/`ctl.bend` automation with reset at placement end; reject track+clip automation of the same parameter — validation side done (conflict + missing `ctl.*` errors; the DAW writes non-flat envelopes so the save is blocked, never lossy); flattening waits for add-unified-synth 6a
- [x] 2.6 Presets `us-patch/1` read/write; preset round trip test
- [x] 2.7 `song` binary: `export`, `import`, `render` (WAV 16/32f via `Player`), `info`, `check`; verify `render` == player render of the exported blob
- [x] 2.8 Song `meta` (author, description, copyright, url; optional title, license, adapted_from): model, schema, validation (an adapted work says so), `song import --meta`, `song info`; `float_roundtrip` so fractional ticks save exactly. Verify: the "Song metadata" scenarios; `tools/demo-songs` writes the six reference songs to `project/music`

## 3. Live host `crates/synth-wasm` (synth-live-host)

- [x] 3.1 C-ABI skeleton (alloc/dealloc/out_ptr/out_len like `crates/wasm`); build to wasm32 from `packages/daw/build.ts`
- [x] 3.2 Tests first (native): live vs `Player` bit-identity on a 3-track song with automation and sidechain, 128-frame calls
- [x] 3.3 `ScheduleSource`, transport (play, stop + tails, loop, seek with chase), render without allocation (test with a counting allocator)
- [x] 3.4 Edits per D4: schedule swap, `set_param`, per-track device rebuild, mixer rebuild; edit fuzz test (random edits between calls never panic)
- [x] 3.5 Audition note-on/off; per-track and master peaks; load ratio
- [x] 3.6 Export parameter tables as JSON; expose `song` validate/export/import/preset through the C ABI
- [x] 3.7 Measure: worst-case render time per 128 frames on a dense song in Chrome; decide on the Worker+SAB fallback (D6) from numbers

## 4. Server (synth-daw: serving and file access)

- [x] 4.1 Tests first in `packages/cli/src/server.test.ts`: `/daw` token injection, `/api/music` list/read/write, `..` and symlink escape → 403, foreign Origin, size limit, invalid song not written
- [x] 4.1a Tests first: `/api/library` lists sources with counts and skips missing ones; `/api/bank` 404 without a bank, bytes with `GM_DLS`
- [x] 4.1b Catalog builder (from `mockup/gen-catalog.py`): project presets, GM bank via `dls`, Tunefish programs, WaveSabre song instruments, repo patches; dedupe WaveSabre instruments by chunk bytes; names from `.als`/`.xrns` when present
- [x] 4.2 Implement `/daw` and `/api/music` (confine, extension whitelist, `song check` before write, atomic write)

## 5. DAW app `packages/daw` (synth-daw)

- [x] 5.1 Package scaffold (`build.ts`, `index.html`, `style.css` with the shared tokens), splitters (reuse shader-viewer's), engine wrapper: AudioWorklet + wasm (D6), worker for bounce (D7)
- [x] 5.2 Song store + command stack (undo/redo, drag = one step), dirty state, open/new/import/save/export, validation error list
- [x] 5.3 Transport bar (play/stop, loop, position, tempo, signature, follow, snap, load, "edited since play" badge)
- [x] 5.4 Arrangement canvas: ruler + seek, tracks, clips (create/move/resize/loop/duplicate/link/unique/delete), zoom/scroll, automation lanes (point add, drag with snap and neighbour clamp, delete by double-click/right-click/Delete, keyboard move, one undo step per gesture)
- [x] 5.4a Arrangement editing: clip selection (click/Shift/marquee/Ctrl+A/Esc), insert cursor, drag move incl. other track, edge resize, arrows, cut/copy/paste/linked paste/duplicate/delete, overlap trimming, Align (Q); tests for overlap resolution and relative-track paste
- [x] 5.5 Piano roll: keys + audition, draw/select/move/resize/delete/transpose/quantize, velocity lane
- [x] 5.4d Transport UI: ruler loop strip (move/resize/draw, snap), Loop field, L/Ctrl+L, song length auto-grow + Length field + after-end shading, seek, playback display (playhead in ruler/lanes/roll, follow, wrap, clip and automation values, meters), Stop twice to the loop start, edited-since-play badge; clip body = range select, title bar = move
- [x] 5.6b Device values: knob writes to the patch/effect state, preset edited marker, envelope redraw, preset load replaces values; knobs follow automation at the playhead (play, seek, point edit) with hand-override + Re-enable automation; tests
- [x] 5.4e Zoom: arrangement px/bar and roll px/step + px/key as view state; Ctrl/⌘+wheel, pinch (wheel ctrlKey + Safari gesture events), ruler wheel, buttons, ± keys, Shift+Z fit; pointer-anchored; ruler label thinning; user-select none on chrome; tests for anchor and hit-testing at zoom; Follow pauses on manual scroll/zoom (resume on F, play, seek); single scroller for lanes/headers and roll/keys
- [x] 5.4f Track and clip commands: track menu (add, rename/F2, duplicate, Alt+↑↓, Ctrl+Delete), name marks, clip create/open by double-click, clip menu, make unique, loop clips (Alt+drag, repeat in playback/export/bounce); effect reorder/remove with lane remap; computer-keyboard mode (M); song switching prompt; tests
- [x] 5.4b Note editing: selection (click/Shift/marquee), insert cursor, drag move/resize, double-click add/delete, arrows/Alt-arrows, transpose buttons, cut/copy/paste/duplicate/delete, quantize with strength and lengths, legato, pitch scrolling centred on the clip; shared undo stack and typed clipboards; tests per command
- [x] 5.4c Clip-owned notes: linked placements share notes, roll spans the clip length with clip-relative loop/playhead, thumbnails render the notes and refresh on edit, clips carry notes on copy/move; tests
- [ ] 5.5a Controller lane: velocity: one stem per note (chord stems side by side, hover outlines the note), single-note drag, group-relative drag on selection (Alt = equal, Ctrl/⌘ = one note), sweep paint, wheel nudge, Vel field, undo per gesture; mod-wheel and pitch-bend clip envelopes (points, bend centre snap); audition MW/PB controls + Web MIDI wheel/bend — velocity editing done; wheel/bend envelopes are kept in the song but reach the engine only with add-unified-synth 6a
- [x] 5.6 Device panel generated from tables: knobs/selects with physical values, groups, envelope/filter visuals, structural rebuild, presets, effect chain add/remove/reorder/bypass
- [x] 5.6a Slot racks (osc/filter/env/LFO/mod): used-slot view, add, remove-compact, move-swap, one command that also rewrites mod targets and automation lanes; tests for the retarget mapping
- [x] 5.7 Mixer: strips, volume dB, mute/solo (solo keeps send sources), meters; inserts per strip incl. bus and master (same data as the device chain); post-fader sends stored as receives on the target, sidechain sends, "+ send" excludes tracks that feed this one; "+ Bus" (no-instrument track)
- [x] 5.7a Track selection (header, clip, strip; Enter, Arrow Up/Down) rebuilds device panel, piano roll and context line
- [ ] 5.7b Library panel: search, per-source counts, inspector, audition, load into the selected track via `synth-convert`, licence notes; GM sampler editor with source select, GM picker and key-region map — search, counts, inspector, audition, presets and GM sampler load done; Tunefish/WaveSabre entries wait for synth-convert instrument conversion
- [x] 5.8 Computer keyboard + Web MIDI audition on the armed track
- [x] 5.9 Offline bounce (whole song / loop) with progress and cancel
- [x] 5.10 Browser tests (headless Chrome like shader-viewer): draw note → heard (rendered buffer non-silent at the note), knob changes output, undo, save round trip, bounce == `song render`, keyboard-only knob operation, axe a11y check

- [x] 5.11 Usability round: loop off by default; Follow resumes at the right edge after a hand scroll; tracker-layout computer keyboard on the selected track (arming follows selection); automation record (●, Shift+R, touch overwrite, lane created and shown, one undo step); piano roll draws only the visible part (long clips); automated tracks show their lanes on load; engine trap reported and restartable

- [x] 5.12 GSM samples: library source of samples stored in project songs (play, load); import an audio file as GSM (libgsm encoder port in crates/gsm, engine `gsm_encode`/`gsm_decode`); remove unused samples
- [x] 5.13 Live device graphics: envelope curve follows its knobs (engine-accurate per mode); sampler settings as named controls with a draggable waveform; global undo shortcut

## 6. Docs and close-out

- [x] 6.1 `docs/synth/daw.md` (formats, architecture diagram, live vs bounce, shortcuts); `docs/architecture.md` audio section; `README.md` how to start the DAW
- [x] 6.2 `AGENTS.md` rows: `crates/AGENTS.md`, `crates/song/AGENTS.md`, `crates/synth-wasm/AGENTS.md`, `packages/AGENTS.md`, `packages/daw/AGENTS.md`, `schema/`
- [x] 6.3 Example song `project/music/demo.song.json` + two presets; `song render` of it in CI-style test
- [x] 6.4 Code review and code-quality gates; `openspec validate add-synth-daw` (CodeRabbit on tracked + new files: 10 findings, 9 fixed, 1 false positive pinned by a test; rustfmt, tsc clean)
