# tools/song-page (sidecar: full detail of the AGENTS.md row)

Pull-only (not auto-injected).

## Files
- `build.ts [out]` (default `tools/song-page/out`, git-ignored): listening page for every `*.song.json` under `project/music/**` (demo + released). Needs `target/release/song`. Per song: `song export` → `songs/<slug>.bin`, cache by song-file sha256 in `cache.json`, length from `song check` frames / 44100; stale blobs + cache entries dropped (not with `ONLY`). Copies `buildSynthWasm(true)` (`packages/cli/src/synth.ts`) to `synth.wasm`; writes `songs.json` + static `index.html`. URLs carry `?v=<sha256:10>` of content (Pages caches 10 min). Env: `BANK_URL` (default `../daw/data/bank.dls`), `JOBS` (8), `CONTACT` (removal contact shown on page), `ONLY=<regex on path>`.
- Exports: `songFiles`, `groupOf` (Demo songs / 64K intros and executable music (WaveSabre) / The Black Lotus, 1997 / Farbrausch productions / V2M collection (via modland)), `slugOf`, `needsBank`, `GROUPS`, `page`, type `Entry`.
- `needsBank(song)`: some osc type sampler (1/3) with `s7` ≥ 0.5 (DLS) and sample > 0; only those songs fetch the GM bank (17 WaveSabre songs when written).
- Page: render `WORKER` (engine `bounce_open`/`bounce_step` → 1-s int16 chunks, `trunc(fround(x·32767))` = `song render` bytes; demo bit-identical, engage 28 samples ±1 wasm vs native) over a MessageChannel to the `WORKLET` player (plays rendered part, silence + "waiting" past it, instant seek inside it). Sticky play/seek bar with rendered fraction, search, credits, `#slug` / `#slug@s`, keys Space ←/→ (±10 s) N P.
- `serve.ts [dir] [port]` (default out, 6480): byte ranges, `application/wasm`, serves `GM_DLS` (default `~/Downloads/64k-tools/synth-lab/ws/gm.dls`) at `/daw/data/bank.dls`.
- `build.test.ts`: slugs unique + known groups, bank songs (engage, atlas yes; demo, v2 no), escaping closes script once, page plays synth blobs not `<audio>`.

## History
- See change: add-unified-synth.
