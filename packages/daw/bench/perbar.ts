// Diagnostic: render time per bar of a song in Chrome (main-thread wasm, live host, 128-frame calls).
// SONG=project/music/x.song.json [MUTE=id,id] [FROM=bar TO=bar]
import { chromium } from "playwright";
import { startServer } from "../../cli/src/server.ts";
import { DIST } from "../build.ts";
const song = JSON.parse(await Bun.file(process.env.SONG!).text());
for (const id of (process.env.MUTE ?? "").split(",").filter(Boolean)) {
  const t = song.tracks.find((t: any) => t.id === id); if (t) { t.devices = []; t.lanes = []; }
}
for (const spec of (process.env.KEEP ?? "").split(",").filter(Boolean)) {  // KEEP=t28:1 keeps the first device(s) only
  const [id, n] = spec.split(":"), t = song.tracks.find((t: any) => t.id === id);
  if (t) { t.devices = t.devices.slice(0, +n); t.lanes = t.lanes.filter((l: any) => l.device < +n); }
}
const s = await startServer({ port: 0, build: false, dawDist: DIST, headless: false });
const b = await chromium.launch({ channel: "chrome" });
const p = await b.newPage();
await p.goto(`${s.url}daw/`);
await p.waitForFunction(() => (window as any).DAW?.engine);
const r = await p.evaluate(([json, to, RESTART]) => {
  const E = (window as any).DAW.engine, x = E.x, sg = JSON.parse(json);
  const ex = E.exportBlob(json); if (!ex.blob) return { error: JSON.stringify(ex.errors) };
  const ptr = x.alloc(ex.blob.length); new Uint8Array(x.memory.buffer, ptr, ex.blob.length).set(ex.blob);
  if (!x.load(ptr, ex.blob.length)) return { error: "load failed" };
  x.play(0);
  const bar = sg.rate * 60 / sg.tempo * sg.signature[0], out: number[] = [];
  let f = 0, acc = 0;
  for (let k = 0; k < to; k++) {
    const end = (k + 1) * bar; acc = 0; let n = 0;
    while (f < end) { const t0 = performance.now(); x.render(128); acc += performance.now() - t0; f += 128; n++; }
    out.push(acc / n / (128 / sg.rate * 1000) * 100);
    if (k + 1 === Number(RESTART)) { x.stop(); x.play(f); }  // RESTART=bar: stop + play at the same spot
  }
  return { tempo: sg.tempo, out };
}, [JSON.stringify(song), Number(process.env.TO ?? 60), process.env.RESTART ?? "0"] as const);
if ((r as any).error) console.log(r); else console.log((r as any).out.map((v: number, i: number) => `${i + 1}:${v.toFixed(0)}`).join(" "));
await b.close(); await s.stop(); process.exit(0);
