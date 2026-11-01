# unisynth

One soft synth for a 64k intro, by **robson / Exceed**. Four synth families from the demoscene, rebuilt as settings of
the same voice and checked against the originals.

- **Site:** <https://unisynth.exceed.hu/>
- **DAW in the browser:** <https://unisynth.exceed.hu/daw/>

## This repository

| Path | What |
|---|---|
| `site/` | The page served by GitHub Pages (`site/CNAME` = `unisynth.exceed.hu`) |
| `site/daw/` | Static, read-only build of the DAW (WebAssembly synth + editor + song snapshot) |
| `scripts/build-daw.sh` | Rebuilds `site/daw/` from the current source tree |
| `.github/workflows/pages.yml` | Deploys `site/` on every push to `main` |

The synth, the player (`synth-core`), the converter (`synth-convert`) and the DAW sources still live in the intro's
working tree and move here in a later step.

## Running the page locally

```bash
cd site && python3 -m http.server 8000   # http://localhost:8000/ and /daw/
```

GitHub Pages cannot send the COOP/COEP headers, so the DAW bounces single-threaded there (same output, slower).

## Licence

Code: GPL-3.0-or-later (see `LICENSE`; parts are ported from GPL-licensed Tunefish). The page text, films and
artwork: © robson / Exceed, all rights reserved. Songs, team logos and third-party material: see `NOTICE.md`.
