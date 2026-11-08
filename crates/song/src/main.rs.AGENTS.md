# crates/song/src/main.rs (sidecar: full detail of the AGENTS.md row)

Pull-only (not auto-injected).

## Commands
- `song export <song.json> -o <blob>`: validate + export.
- `song import <blob> -o <song.json> [--meta <meta.json>] [--one-clip]`: blob → song/2; lanes cut into patterns unless `--one-clip` (`import_one_clip`). `--meta` goes through `apply_meta` (authorship, track names, `about`), then `validate`; issues printed with the meta path, nothing written.
- `song render <song.json|blob> -o <wav> [--f32] [--bank gm.dls|plus] [--threads <n>]`: 16-bit WAV by default. `--threads` n ≥ 1 (0 / non-number / missing count → error, nothing written); needs feature `threads` for n > 1 ("built without the `threads` feature").
- `song info <song.json|blob>`: title/author/copyright/url, `adapted from …`, description; song `about` (type; style; tempo); counts; one line per track with devices (`[role]` from track `about`); `features:`.
- `song check <song.json|blob> [--bank]`: validate, export, load into the player (`render::load`, no render); prints `<file>: loads (<format>, <frames> frames)` (`format` = file's `format` field or `blob`).
- `song migrate <file> [-o out]`: a `/1` song or `*.patch.json` preset saved as `/2` (in place without `-o`); a `/2` file is written back unchanged.
- `song`/`help`/`--help`/`-h` print usage (exit 0 for help, 2 otherwise).

## Behaviour
- Bank: `--bank <path>` or env `GM_DLS`; empty = none; `plus` generates the gmplus procedural bank (`gmgen::plus::build`, change add-gmplus-bank).
- Errors `file: /pointer: reason`, exit 1. Atomic write (tmp + rename); nothing written on failure.
- A JSON input is recognised by `.json` suffix or a leading `{`; otherwise a blob.

See change: add-import-patterns, add-synth-threads, unify-v2-algorithms.
