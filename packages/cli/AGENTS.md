# packages/cli

DAW dev server (`@unisynth/cli`), split from the 64k control server (editor/WebSocket parts dropped). Serves `packages/daw/dist` at `/daw/` and the file API confined to `project/music`. Run: `bun packages/cli/src/cli.ts serve [--port 6464] [--build]`. Tests: `bun test packages/cli`.

| File | Purpose |
|---|---|
| `package.json` | `@unisynth/cli`; bin `unisynth` → `src/cli.ts`; dep `playwright`. |
| `src/cli.ts` | `serve [--port] [--build]` → `startServer`, prints `DAW: <url>daw/`; other args print usage (exit 0 for none/help, else 1). |
| `src/music.test.ts` | `musicApi` contract: list + read; atomic valid write, invalid refused with paths and not written; paths outside `project/music`, symlinks out, other extensions refused; `MAX_BYTES` refusal; library sources + counts, GSM samples once per distinct sample, converted entries only from `us-library/2`; `/api/bank` 404 without bank; `/daw` token meta; foreign Origin cannot write. |
| `src/music.ts` | File API: `GET /api/music` list (songs, presets, blobs), `?path=` read, `PUT` write (`*.song.json`/`*.patch.json` validated by the engine module, 422 + issues; atomic rename); `GET /api/library` (`library`, `gsmSamples`, converted `conv` from `project/music/.library.json` `us-library/2`), `GET /api/bank` (`bankPath`: `GM_DLS` or OS bank, 404 without). `confineMusic(root, p)` (no `..`, absolute, symlink out; extensions `.song.json`/`.patch.json`/`.song.bin`), `MAX_BYTES` 16 MiB, `musicApi(root, req, url, synth)`.; `tunefish3` source (programs `{n, name, conv}` listed from `.library.json` itself, converted ones only; `counts.tunefish3`; add-tf3-import 6.5) |
| `src/server.ts` | `startServer({root,port,build,dawDist,headless})` → `{url,port,stop}`; `ROOT`, `DEFAULT_PORT` 6464. 127.0.0.1; `/` → `/daw/`; page with token meta `x-64k-token`, files from `dawDist` with COOP/COEP/CORP (`ISOLATED`); `/api/*` with a foreign Origin → 403; `headless` accepted, ignored. |
| `src/synth.ts` | Engine module loader in Bun: `buildSynthWasm(force)` → `WASM` (cargo wasm32), `buildSynthWasmMt(force)` → `WASM_MT` (threaded: shared memory, atomics, `MT_MAX_MEMORY` 1 GiB), `loadSynth(path)` → `Synth` {`validateSong`, `validatePreset`, `bankInfo`}, cached. |
