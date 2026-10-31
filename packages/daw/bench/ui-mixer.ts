// UI cost of the Mixer tab (change daw-solid-ts): main-thread ms per edit and frame times while playing, in Chrome.
// Compares page builds: DIST=<packages/daw/dist> ROOT=<repo with the song> SONG=project/music/x.song.json
//   select  — click a strip name (selectTrack → renderAll), mean ms of the synchronous handler
//   fader   — one volume input event on a strip fader, mean ms
//   mute    — click a strip's M button, mean ms
//   play    — 5 s playing with the mixer shown: mean / p95 / max frame interval, frames > 20 ms
import { chromium } from "playwright";
import { startServer } from "../../cli/src/server.ts";

const dist = process.env.DIST ?? new URL("../dist", import.meta.url).pathname;
const s = await startServer({ port: 0, root: process.env.ROOT, build: false, dawDist: dist, headless: false });
const b = await chromium.launch({ channel: "chrome", args: ["--autoplay-policy=no-user-gesture-required"] });
const p = await b.newPage({ viewport: { width: 1600, height: 1000 } });
p.on("pageerror", (e) => console.log("pageerror", e.message));
await p.goto(`${s.url}daw/?song=${process.env.SONG ?? "project/music/demo.song.json"}&tab=mix`);
await p.waitForSelector("#mixer .strip");
await p.waitForTimeout(1000);
const r = await p.evaluate(async () => {
  const mean = (a: number[]) => a.reduce((x, y) => x + y, 0) / a.length;
  const time = (n: number, f: (i: number) => void) => { const ms: number[] = []; for (let i = 0; i < n; i++) { const t = performance.now(); f(i); ms.push(performance.now() - t); } return mean(ms); };
  const strips = () => [...document.querySelectorAll<HTMLElement>("#mixer .strip")];
  const n = strips().length;
  const select = time(40, (i) => strips()[i % n].querySelector<HTMLButtonElement>(".sn")!.click());
  const f = strips()[0].querySelector<HTMLInputElement>(".fader")!;
  const v0 = +f.value;
  const fader = time(200, (i) => { f.value = String(v0 - (i % 20) * 0.1); f.dispatchEvent(new Event("input", { bubbles: true })); });
  f.value = String(v0); f.dispatchEvent(new Event("input", { bubbles: true })); f.dispatchEvent(new Event("change", { bubbles: true }));
  const mute = time(40, () => strips()[1].querySelector<HTMLButtonElement>('button[aria-label^="Mute "]')!.click());
  document.dispatchEvent(new KeyboardEvent("keydown", { key: " ", code: "Space", bubbles: true }));
  await new Promise((res) => setTimeout(res, 1000));
  const iv: number[] = [];
  await new Promise<void>((res) => { let last = performance.now(); const t0 = last; const f2 = (now: number) => { iv.push(now - last); last = now; if (now - t0 < 5000) requestAnimationFrame(f2); else res(); }; requestAnimationFrame(f2); });
  document.dispatchEvent(new KeyboardEvent("keydown", { key: " ", code: "Space", bubbles: true }));
  iv.sort((a, b) => a - b);
  return { strips: n, select, fader, mute, frame: { mean: mean(iv), p95: iv[Math.floor(iv.length * 0.95)], max: iv[iv.length - 1], over20: iv.filter((x) => x > 20).length, n: iv.length } };
});
const f = (x: number) => x.toFixed(2);
console.log(`strips ${r.strips}  select ${f(r.select)} ms  fader ${f(r.fader)} ms  mute ${f(r.mute)} ms  | play frames ${r.frame.n}: mean ${f(r.frame.mean)} p95 ${f(r.frame.p95)} max ${f(r.frame.max)} >20ms ${r.frame.over20}`);
await b.close();
await s.stop();
