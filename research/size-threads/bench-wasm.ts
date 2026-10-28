// add-wasm-threads task 4.4: DAW bounce time in Chrome, single-thread vs threaded (2/4/8 workers), whole songs.
//   bun research/size-threads/bench-wasm.ts [songs…]   (needs packages/daw built; prints a markdown table)
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { chromium } from "../../packages/daw/node_modules/playwright/index.mjs";
import { startServer } from "../../packages/cli/src/server.ts";
import { buildDaw, DIST } from "../../packages/daw/build.ts";

const ROOT = join(import.meta.dir, "../..");
const songs = process.argv.slice(2).length ? process.argv.slice(2) : ["demo", "eladamri", "elysian", "engage"];
const counts = [1, 2, 4, 8];
await buildDaw({ wasm: false });
const s = await startServer({ port: 0, root: ROOT, build: false, dawDist: DIST, headless: false });
const b = await chromium.launch({ channel: process.env.CHROME_CHANNEL ?? "chrome", headless: true });
const p = await b.newPage();
await p.goto(`${s.url}daw/`);
await p.waitForFunction(() => (window as any).DAW?.engine);
console.log(`| Song | ${counts.map((c) => `${c} worker${c > 1 ? "s" : ""}`).join(" | ")} | best speed-up | equal |`);
console.log(`|---|${counts.map(() => "---").join("|")}|---|---|`);
for (const song of songs) {
  const json = readFileSync(join(ROOT, `project/music/${song}.song.json`), "utf8");
  const r = await p.evaluate(async ([json, counts]) => {
    const E = (window as any).DAW.engine, ex = E.exportBlob(json), out: { n: number; ms: number; used: number; hash: string }[] = [];
    for (const n of counts) {
      E.mt = { workers: n };
      const t0 = performance.now();
      const smp: Float32Array = await E.bounce(ex.blob, null, () => {}).done;
      const ms = performance.now() - t0;
      const d = new Uint8Array(await crypto.subtle.digest("SHA-256", smp));
      out.push({ n, ms, used: E.lastBounceWorkers, hash: [...d].slice(0, 8).join(",") });
    }
    return out;
  }, [json, counts] as const);
  const best = Math.min(...r.map((x) => x.ms));
  const equal = r.every((x) => x.hash === r[0].hash) && r.every((x) => x.used === x.n);
  console.log(`| \`${song}\` | ${r.map((x) => (x.ms / 1000).toFixed(1)).join(" | ")} | ${(r[0].ms / best).toFixed(2)}× | ${equal ? "yes" : "NO"} |`);
}
await b.close();
await s.stop();
