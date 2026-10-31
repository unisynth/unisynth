// UI cost of the DAW in Chrome (change daw-solid-ts, design D8): main-thread ms of synchronous handlers and frame
// intervals while playing. Run before and after a port step; compare medians of 3 runs.
// DIST=<packages/daw/dist> ROOT=<repo holding SONG> SONG=project/music/x.song.json  bun packages/daw/bench/ui.ts
//   select-dev  click a track header with the Device tab shown (selectTrack → device panel), mean ms
//   select-mix  click a strip name with the Mixer tab shown, mean ms
//   knob        Arrow Up on the first instrument knob (edit + commit + livePush schedule), mean ms
//   fader       one input event on a strip fader, mean ms
//   mute        click a strip's M button, mean ms
//   undo        Ctrl+Z after a knob step, mean ms
//   roll        show the Piano roll tab on the track with the most notes, ms
//   play-mix / play-dev  5 s playing: mean, p95, max frame interval (ms), frames > 20 ms
import { chromium } from "playwright";
import { startServer } from "../../cli/src/server.ts";

const dist = process.env.DIST ?? new URL("../dist", import.meta.url).pathname;
const s = await startServer({ port: 0, root: process.env.ROOT, build: false, dawDist: dist, headless: false });
const b = await chromium.launch({ channel: "chrome", args: ["--autoplay-policy=no-user-gesture-required"] });
const p = await b.newPage({ viewport: { width: 1600, height: 1000 } });
p.on("pageerror", (e) => console.log("pageerror", e.message));
await p.goto(`${s.url}daw/?song=${process.env.SONG ?? "project/music/demo.song.json"}`);
await p.waitForSelector(".thdr.track");
await p.waitForTimeout(1000);

const r = await p.evaluate(async () => {
  const mean = (a: number[]) => a.reduce((x, y) => x + y, 0) / a.length;
  // handler + the microtasks it queues (knob commits run in a microtask)
  const time = async (n: number, f: (i: number) => void, pre?: () => void) => {
    const ms: number[] = [];
    for (let i = 0; i < n; i++) { pre?.(); await Promise.resolve(); const t = performance.now(); f(i); await Promise.resolve(); await Promise.resolve(); ms.push(performance.now() - t); }
    return mean(ms);
  };
  const tab = (id: string) => document.getElementById(id)!.click();
  const key = (el: EventTarget, k: string, o: KeyboardEventInit = {}) => el.dispatchEvent(new KeyboardEvent("keydown", { key: k, bubbles: true, ...o }));
  const frames = async () => {
    const iv: number[] = [];
    await new Promise<void>((res) => { let last = performance.now(); const t0 = last; const f = (now: number) => { iv.push(now - last); last = now; if (now - t0 < 5000) requestAnimationFrame(f); else res(); }; requestAnimationFrame(f); });
    iv.sort((a, b) => a - b);
    return { mean: mean(iv), p95: iv[Math.floor(iv.length * 0.95)], max: iv[iv.length - 1], over20: iv.filter((x) => x > 20).length };
  };
  const out: Record<string, unknown> = {};
  const hdrs = () => [...document.querySelectorAll<HTMLElement>(".thdr.track")];
  const n = hdrs().length;
  const inst = hdrs().findIndex((h) => !h.classList.contains("bus"));

  tab("tab-dev");
  out["select-dev"] = await time(20, (i) => hdrs()[i % n].querySelector<HTMLElement>(".name, .tname, span")?.click() ?? hdrs()[i % n].click());
  hdrs()[inst].click();
  const knobEl = () => document.querySelector<HTMLElement>("#p-dev .knob")!;
  out.knob = await time(50, () => key(knobEl(), "ArrowUp"));
  const v = () => knobEl().getAttribute("aria-valuenow");
  const before = v(); key(knobEl(), "ArrowUp"); await Promise.resolve(); await Promise.resolve();
  const stepped = v(); key(document.body, "z", { ctrlKey: true });
  if (stepped === before || v() !== before) throw new Error(`bench: knob step/undo had no effect (${before} → ${stepped} → ${v()})`);
  out.undo = await time(20, () => key(document.body, "z", { ctrlKey: true }), () => key(knobEl(), "ArrowUp"));

  tab("tab-mix");
  const strips = () => [...document.querySelectorAll<HTMLElement>("#mixer .strip")];
  out["select-mix"] = await time(20, (i) => strips()[i % strips().length].querySelector<HTMLButtonElement>(".sn")!.click());
  const f = strips()[0].querySelector<HTMLInputElement>(".fader")!, v0 = +f.value;
  out.fader = await time(200, (i) => { f.value = String(v0 - (i % 20) * 0.1); f.dispatchEvent(new Event("input", { bubbles: true })); });
  f.value = String(v0); f.dispatchEvent(new Event("input", { bubbles: true })); f.dispatchEvent(new Event("change", { bubbles: true }));
  out.mute = await time(20, () => strips()[1].querySelector<HTMLButtonElement>('button[aria-label^="Mute "]')!.click());

  const song = (window as any).DAW.debug.song();
  const notes = (id: string) => song.clips.filter((c: any) => song.tracks.find((t: any) => t.id === id)?.placements?.some((pl: any) => pl.clip === c.id)).reduce((a: number, c: any) => a + (c.notes?.length ?? 0), 0);
  const busiest = song.tracks.map((t: any) => t.id).sort((a: string, b: string) => notes(b) - notes(a))[0];
  document.querySelector<HTMLElement>(`.thdr.track[data-track="${busiest}"]`)!.click();
  tab("tab-dev");
  out.roll = await time(1, () => tab("tab-roll"));

  tab("tab-mix");
  key(document.body, " ", { code: "Space" });
  await new Promise((res) => setTimeout(res, 800));
  out["play-mix"] = await frames();
  tab("tab-dev");
  out["play-dev"] = await frames();
  key(document.body, " ", { code: "Space" });
  return out;
});
const f2 = (x: number) => x.toFixed(2);
const fr = (x: any) => `mean ${f2(x.mean)} p95 ${f2(x.p95)} max ${f2(x.max)} >20 ${x.over20}`;
console.log(Object.entries(r).map(([k, v]) => `${k} ${typeof v === "number" ? f2(v) : fr(v)}`).join("  |  "));
await b.close();
await s.stop();
