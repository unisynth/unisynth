// add-wasm-threads task 1.1 spike runner: serves web/ with COOP/COEP and runs it in headless Chrome.
//   bun research/wasm-threads-spike/run.ts
import { join } from "node:path";
const dir = import.meta.dir;
const wasm = join(dir, "target/wasm32-unknown-unknown/release/wasm_threads_spike.wasm");
const srv = Bun.serve({
  port: 0,
  fetch(req) {
    const p = new URL(req.url).pathname;
    if (p === "/favicon.ico") return new Response(null, { status: 204 });
    const f = p === "/spike.wasm" ? wasm : join(dir, "web", p === "/" ? "index.html" : p.slice(1));
    const type = f.endsWith(".wasm") ? "application/wasm" : f.endsWith(".js") ? "text/javascript" : "text/html";
    return new Response(Bun.file(f), { headers: { "content-type": type, "cross-origin-opener-policy": "same-origin", "cross-origin-embedder-policy": "require-corp", "cross-origin-resource-policy": "same-origin" } });
  },
});
const { chromium } = await import(join(dir, "../../packages/daw/node_modules/playwright/index.mjs"));
const browser = await chromium.launch({ channel: "chrome", headless: true });
const page = await browser.newPage();
const errs: string[] = [];
page.on("pageerror", (e: Error) => errs.push(e.message));
page.on("console", (m: any) => m.type() === "error" && errs.push(m.text()));
await page.goto(`http://127.0.0.1:${srv.port}/`);
await page.waitForFunction(() => document.title === "done", null, { timeout: 30_000 }).catch(() => {});
console.log(JSON.stringify(await page.evaluate(() => (window as any).RESULT), null, 1), errs);
await browser.close();
srv.stop();
