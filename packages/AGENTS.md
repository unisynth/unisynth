# packages

pnpm workspace (bun runs scripts/tests). Do not `bun install`.

| Package | Purpose |
|---|---|
| `cli/` | DAW dev server (split from the 64k control server; editor/WebSocket parts dropped). `src/server.ts` `startServer({root,port,build,dawDist})` → `{url,port,stop}`: 127.0.0.1, `/` → `/daw/`, DAW page (token meta `x-64k-token`) + files from `packages/daw/dist` (`dawDist` override) with COOP/COEP/CORP (`ISOLATED`), `/api/*` refuses foreign Origin; `headless` accepted and ignored. `src/music.ts` `/api/music`, `/api/library`, `/api/bank` (confined to `project/music`). `src/synth.ts` engine module loader (`buildSynthWasm`, `loadSynth`). `src/cli.ts` `serve [--port] [--build]`. |
| `daw/` | Browser DAW at `/daw` (change add-synth-daw): mockup UI ported onto the real `song/1` model, engine module (`crates/synth-wasm`: main-thread song functions, AudioWorklet live host, bounce Worker), device panels generated from the engine's parameter tables, `/api/music` files, `/api/library`. `build.ts` → `dist/`. → see `daw/AGENTS.md`. |
