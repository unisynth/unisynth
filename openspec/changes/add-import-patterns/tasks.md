# Tasks

## 1. Patterns in import

- [x] 1.1 Tests first: export(patterned) == export(one clip) byte-identical (demo songs, synthetic), no cut through a note, repeats share a clip, looping merge, variant naming, shared lane linked, `--one-clip`
- [x] 1.2 `crates/song/src/patterns.rs` (P1–P4); `import` default patterned, `import_one_clip`; CLI `--one-clip`
- [x] 1.3 Verify on all 52 WaveSabre lab songs (after `synth-convert ws`; `PATTERN_BLOBS`) and the demo songs: byte-identical export

## 2. Names and descriptions

- [x] 2.1 `About` (song) and `TrackAbout` (track) in model + schema + validation (not blank); never exported
- [x] 2.2 `--meta` config: `about`, `tracks: [{name, about}]` (count must match); tests
- [x] 2.3 `packages/daw` model keeps `about` on load/save (track name tooltip); model tests on rebuilt songs
- [x] 2.4 `tools/demo-songs/names.ts` (WaveSabre `.ref` names) + `meta/<name>.json`; `build.ts` merges
- [x] 2.5 Per-track and per-song descriptions for the six demo songs (instrument analysis + research), rebuild `project/music`
