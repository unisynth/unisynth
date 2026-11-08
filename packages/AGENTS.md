# packages

pnpm workspace (bun runs scripts/tests). Do not `bun install`.

| Package | Purpose |
|---|---|
| `cli/` | DAW dev server: `/daw/` page + `/api/music`, `/api/library`, `/api/bank` confined to `project/music`; engine module loader. → see `cli/AGENTS.md`. |
| `daw/` | Browser DAW at `/daw` (change add-synth-daw): UI on the `song/2` model, engine module (`crates/synth-wasm`), AudioWorklet host, bounce Worker, build/bench/browser tests. `build.ts` → `dist/`. → see `daw/AGENTS.md`. |
