# scripts

Site publish scripts (bash, `set -euo pipefail`). `SRC` = source tree (default repo root); output into `site/`.

| File | Purpose |
|---|---|
| `build-daw.sh` | Rebuilds `site/daw` via `bun packages/daw/build-static.ts`; publishes `GM_DLS` (default `~/Downloads/64k-tools/synth-lab/ws/gm.dls`; set it explicitly, empty value = none, then the server falls back to the OS bank; see `NOTICE.md`) as `site/daw/data/bank.dls`; cache-busts by `?v=<sha1:10>` on the `main.js` asset names (wasm, worklet, bounce) and the `index.html` refs to `main.js`/`style.css` (Pages caches 10 min). |
| `build-songs.sh` | Rebuilds `site/songs` listening page: `cargo build --release -p song`, `bun tools/song-page/build.ts`, copies `index.html`, `synth.wasm`, `songs/`. Warns when `site/daw/data/bank.dls` is missing (GM.DLS songs then silent). |
