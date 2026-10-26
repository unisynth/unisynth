// Build the DAW (change add-synth-daw): engine module (cargo, wasm32) + page bundle, AudioWorklet and bounce worker
// (Bun.build) into packages/daw/dist, served by packages/cli at /daw.
// Usage: bun packages/daw/build.ts   (also `pnpm -r build`)
import { copyFileSync, mkdirSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { buildSynthWasm } from "../cli/src/synth.ts";

const ROOT = fileURLToPath(new URL("../../", import.meta.url));
export const DIST = join(ROOT, "packages/daw/dist");

export async function buildDaw(opts: { wasm?: boolean } = {}): Promise<void> {
  mkdirSync(DIST, { recursive: true });
  copyFileSync(buildSynthWasm(opts.wasm !== false), join(DIST, "synth.wasm"));
  const src = join(ROOT, "packages/daw/src");
  const r = await Bun.build({
    entrypoints: [join(src, "main.ts"), join(src, "worklet.ts"), join(src, "bounce-worker.ts")],
    outdir: DIST,
    target: "browser",
    format: "esm",
    sourcemap: "inline",
  });
  if (!r.success) throw new Error(r.logs.map(String).join("\n"));
  for (const f of ["index.html", "style.css"]) copyFileSync(join(src, f), join(DIST, f));
}

if (import.meta.main) {
  await buildDaw();
  console.log(`DAW built into ${DIST}`);
}
