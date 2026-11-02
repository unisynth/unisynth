// CPU profile of 10 knob-step + undo cycles in Chrome (change daw-solid-ts): self time per function, and the time
// spent in Solid store proxy traps attributed to their callers. Finds hot proxy reads and layout costs.
// ROOT=<repo holding SONG> SONG=project/music/x.song.json [N=10 cycles] [KNOB_ONLY=1: knob steps, no undo] [PLAY=1: 3 s playing, Mixer shown] [SCROLL=1: 2 s arrangement scroll, 40 px/frame] [TAB=1: 20 Mixer → Device tab switches (panel remount)] [SELECT=1: 20 track-header clicks, Device shown]  bun packages/daw/bench/ui-profile.ts   (needs dist)
import { chromium } from "playwright";
import { startServer } from "../../cli/src/server.ts";
const s = await startServer({ port: 0, root: process.env.ROOT, build: false, dawDist: new URL("../dist", import.meta.url).pathname, headless: false });
const b = await chromium.launch({ channel: "chrome" });
const p = await b.newPage({ viewport: { width: 1600, height: 1000 } });
await p.addInitScript(() => localStorage.setItem("daw.verifySnap", "0"));  // measure without the snapshot cache check (on under webdriver)
await p.goto(`${s.url}daw/?song=${process.env.SONG}&tab=dev`); await p.waitForSelector(".thdr.track"); await p.waitForTimeout(1000);
const cdp = await p.context().newCDPSession(p);
await cdp.send("Profiler.enable"); await cdp.send("Profiler.setSamplingInterval", { interval: 100 });
await p.evaluate(() => { const h = [...document.querySelectorAll<HTMLElement>(".thdr.track")].find((x) => !x.classList.contains("bus"))!; h.click(); });
if (process.env.PLAY) {  // PLAY=1: profile 3 s of playback with the Mixer tab shown instead
  await p.evaluate(() => document.getElementById("tab-mix")!.click());
  await cdp.send("Profiler.start");
  await p.evaluate(async () => { document.body.dispatchEvent(new KeyboardEvent("keydown", { key: " ", code: "Space", bubbles: true })); await new Promise((r) => setTimeout(r, 3000)); document.body.dispatchEvent(new KeyboardEvent("keydown", { key: " ", code: "Space", bubbles: true })); });
} else await cdp.send("Profiler.start");
if (process.env.SELECT) await p.evaluate(async () => { const raf = () => new Promise((r) => requestAnimationFrame(r)), h = [...document.querySelectorAll<HTMLElement>(".thdr.track")];
  for (let i = 0; i < 20; i++) { h[i % h.length]!.click(); await raf(); } });
else if (process.env.TAB) await p.evaluate(async () => { const raf = () => new Promise((r) => requestAnimationFrame(r));
  for (let i = 0; i < 20; i++) { document.getElementById("tab-mix")!.click(); await raf(); document.getElementById("tab-dev")!.click(); await raf(); await raf(); } });
else if (process.env.SCROLL) await p.evaluate(async () => { const lw = document.getElementById("lanes-wrap")!, t0 = performance.now();
  await new Promise<void>((res) => { const f = () => { lw.scrollTop = (lw.scrollTop + 40) % Math.max(1, lw.scrollHeight - lw.clientHeight); lw.scrollLeft = (lw.scrollLeft + 40) % Math.max(1, lw.scrollWidth - lw.clientWidth); if (performance.now() - t0 < 2000) requestAnimationFrame(f); else res(); }; requestAnimationFrame(f); }); });
else if (!process.env.PLAY) await p.evaluate(async ([N, KNOB]) => { const k = () => document.querySelector<HTMLElement>("#p-dev .knob")!; for (let i = 0; i < N; i++) { k().dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowUp", bubbles: true })); await Promise.resolve(); await Promise.resolve(); if (!KNOB) document.body.dispatchEvent(new KeyboardEvent("keydown", { key: "z", ctrlKey: true, bubbles: true })); } }, [+(process.env.N ?? 10), !!process.env.KNOB_ONLY] as const);
const { profile } = await cdp.send("Profiler.stop");
const self = new Map<string, number>(); const dt = profile.timeDeltas!; const byId = new Map(profile.nodes.map((n: any) => [n.id, n]));
profile.samples!.forEach((id: number, i: number) => { const n: any = byId.get(id); const k = `${n.callFrame.functionName || "(anon)"} ${n.callFrame.url.split("/").pop()}:${n.callFrame.lineNumber + 1}`; self.set(k, (self.get(k) || 0) + (dt[i] || 0) / 1000); });
const parent = new Map<number, any>(); for (const n of profile.nodes as any[]) for (const c of n.children || []) parent.set(c, n);
const via = new Map<string, number>();
profile.samples!.forEach((id: number, i: number) => { let n: any = byId.get(id); if (!["get", "wrap", "getNodes", "setProperty", "isWrappable", "getDataNodes"].includes(n.callFrame.functionName)) return;
  while (n && ["get", "wrap", "getNodes", "isWrappable", "getDataNodes", "(anon)", ""].includes(n.callFrame.functionName) && parent.get(n.id)) n = parent.get(n.id);
  const k = `${n.callFrame.functionName} :${n.callFrame.lineNumber + 1}`; via.set(k, (via.get(k) || 0) + (dt[i] || 0) / 1000); });
console.log("proxy time by caller:\n" + [...via].sort((a, b) => b[1] - a[1]).slice(0, 12).map(([k, v]) => `${v.toFixed(0).padStart(6)} ms  ${k}`).join("\n"));
// inclusive time of named functions (each sample counted once per function on its stack)
const incl = new Map<string, number>();
profile.samples!.forEach((id: number, i: number) => { const seen = new Set<string>(); for (let n: any = byId.get(id); n; n = parent.get(n.id)) { const f = n.callFrame.functionName; if (f && !seen.has(f)) { seen.add(f); incl.set(f, (incl.get(f) || 0) + (dt[i] || 0) / 1000); } } });
console.log("inclusive:\n" + [...incl].sort((a, b) => b[1] - a[1]).slice(0, 20).map(([k, v]) => `${v.toFixed(0).padStart(6)} ms  ${k}`).join("\n"));
console.log([...self].sort((a, b) => b[1] - a[1]).slice(0, 18).map(([k, v]) => `${v.toFixed(0).padStart(6)} ms  ${k}`).join("\n"));
await b.close(); await s.stop();
