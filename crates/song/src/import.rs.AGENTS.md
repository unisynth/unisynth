# crates/song/src/import.rs (sidecar: full detail of the AGENTS.md row)

Pull-only (not auto-injected).

## API
- `import(blob)` = `import_one_clip` + `crate::patterns::split` (CLI default). Same export as `import_one_clip`.
- `import_one_clip(blob)`: one clip per lane (`lane<N>`, N = lane index + 1; a lane shared by tracks = linked placements) placed once at tick 0, length = max(song length, last note end).
- `apply_meta(song, text)`: import config = `Meta` fields + optional `about` (song) + `tracks: [{name, about?}]` (one per track before the master, else error); song unchanged on error.

## Behaviour
- Blob via `BlobM::read` (structure validated by `synth_core::mix::Song::parse`). Never panics on a blob that parses (fuzzed).
- Devices: kind ≤ 39 → "kind N is a legacy device; convert the song with synth-convert first". Kind 64 via `Chunk::parse`: non-default values by name (`unit_snap`), samples base64, chunk text → `Device.speech` (split on NUL). Effects positional → names via the mode table; no trailer (v3 chunks end in the tag).
- Receives keep the blob channel field: `recv::to` → `Channel`, `recv::from` → `send`.
- Ids `t1…`, `master`; names `Track N` / `Master`.
- `notes`: a note-on pairs with the next note-off of its pitch; open notes end at song end. A note ended at the sample where another started just before it (mono legato: on, then off) lasts one tick longer, so the export (offs first at one sample) plays the new note first again. Velocity clamped 1..=127.
- Automation: ticks fractional (exact samples), value = byte/255. Released-intro tolerance: first point < 0 → tick 0; non-monotonic ticks made monotonic; points after the blob end cut at the end with the interpolated value (no song stretch). Values within 1e-6 outside 0..1 snapped (`unit_snap`).

See change: add-import-patterns, add-v2-import, add-tbl-demo-songs, unify-v2-algorithms.
