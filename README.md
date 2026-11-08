# unisynth

One soft synth for a 64k intro, by **robson / Exceed**. Four synth families from the demoscene, rebuilt as settings of
the same voice and checked against the originals.

- **Site:** <https://unisynth.exceed.hu/>
- **DAW in the browser:** <https://unisynth.exceed.hu/daw/>
- **Songs:** <https://unisynth.exceed.hu/songs/> (every converted song, played live by the synth in the browser)

## This repository

| Path | What |
|---|---|
| `site/` | The page served by GitHub Pages (`site/CNAME` = `unisynth.exceed.hu`) |
| `site/daw/` | Static, read-only build of the DAW (WebAssembly synth + editor + song snapshot + GM bank) |
| `site/songs/` | Listening page: song files (`song export`) + the synth's WebAssembly module; plays each song live, GM bank from `site/daw/data/` |
| `crates/unisynth/` | The unified synth: one slot-based instrument + merged effects |
| `crates/synth-core/` | Shared no_std layer: `Device` contract, DSP primitives, song blob `Player` |
| `crates/wavesabre/`, `crates/tfsynth/`, `crates/glorp/` | Legacy engines (WaveSabre, Tunefish 4, Glorp) the unified synth is checked against |
| `crates/gsm/` | GSM 06.10 decoder (WaveSabre samples) |
| `crates/song/`, `crates/synth-convert/` | `song/2` source format + `song` CLI; converter from WaveSabre/Tunefish/V2/XM songs |
| `crates/synth-wasm/`, `packages/daw/` | The DAW: WebAssembly engine module and the SolidJS browser UI |
| `packages/cli/` | Local DAW dev server (`bun packages/cli/src/cli.ts serve` → http://127.0.0.1:6464/daw/) |
| `project/music/` | Song library (demo songs, converted released songs; see `NOTICE.md`) |
| `docs/synth/` | Architecture docs; `openspec/` specs and change history; `research/` surveys and size probes; `tools/` parity and size tools |
| `scripts/build-daw.sh` | Rebuilds `site/daw/` from this tree |
| `scripts/build-songs.sh` | Rebuilds `site/songs/` from this tree (`tools/song-page`) |
| `.github/workflows/pages.yml` | Deploys `site/` on every push to `main` |

The sources and their history were moved here from the 64k intro repository, which now uses these crates as git
dependencies.

## Building

```bash
pnpm install --frozen-lockfile        # JS dependencies (pnpm owns the lockfile; bun runs scripts and tests)
cargo test --workspace                # Rust crates (nightly, see rust-toolchain.toml)
bun test                              # JS tests (DAW, song schema, server)
bun packages/daw/build.ts && bun packages/cli/src/cli.ts serve   # DAW at http://127.0.0.1:6464/daw/
```

## Running the page locally

```bash
cd site && python3 -m http.server 8000   # http://localhost:8000/, /daw/ and /songs/
```

GitHub Pages cannot send the COOP/COEP headers, so the DAW bounces single-threaded there (same output, slower).

## Licence

Code: GPL-3.0-or-later (see `LICENSE`; parts are ported from GPL-licensed Tunefish). The page text, films and
artwork: © robson / Exceed, all rights reserved. Songs, team logos and third-party material: see `NOTICE.md`.
