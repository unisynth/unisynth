// Build the DAW (change add-synth-daw): engine modules (cargo, wasm32; `synth-mt.wasm` = threaded, add-wasm-threads) +
// AudioWorklet, bounce worker and bounce helper (Bun.build) and the page bundle (Vite + SolidJS, vite.config.ts,
// change daw-solid-ts) into packages/daw/dist, served by packages/cli at /daw.
// Usage: bun packages/daw/build.ts   (also `pnpm -r build`)
import { copyFileSync, mkdirSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { build as viteBuild } from "vite";
import { buildSynthWasm, buildSynthWasmMt } from "../cli/src/synth.ts";

const ROOT = fileURLToPath(new URL("../../", import.meta.url));
export const DIST = join(ROOT, "packages/daw/dist");

export async function buildDaw(opts: { wasm?: boolean } = {}): Promise<void> {
  mkdirSync(DIST, { recursive: true });
  copyFileSync(buildSynthWasm(opts.wasm !== false), join(DIST, "synth.wasm"));
  // threaded engine for the bounce on cross-origin isolated pages (change add-wasm-threads)
  copyFileSync(buildSynthWasmMt(opts.wasm !== false), join(DIST, "synth-mt.wasm"));
  const src = join(ROOT, "packages/daw/src");
  const r = await Bun.build({
    entrypoints: [join(src, "worklet.ts"), join(src, "bounce-worker.ts"), join(src, "bounce-helper.ts")],
    outdir: DIST,
    target: "browser",
    format: "esm",
    sourcemap: "inline",
  });
  if (!r.success) throw new Error(r.logs.map(String).join("\n"));
  await viteBuild({ configFile: join(ROOT, "packages/daw/vite.config.ts") });
  for (const f of ["index.html", "style.css"]) copyFileSync(join(src, f), join(DIST, f));
}

if (import.meta.main) {
  await buildDaw();
  console.log(`DAW built into ${DIST}`);
}
