// Worst-case live render time per 128-frame call on a dense song, in Chrome (task 3.7, design D6). Runs the engine
// module on the page's main thread (performance.now is precise there; the AudioWorklet only has Date.now) through the
// same live-host exports the worklet calls. Usage: bun packages/daw/bench/dense.ts  (needs packages/daw/dist)
import { chromium } from "playwright";
import { startServer } from "../../cli/src/server.ts";
import { DIST, buildDaw } from "../build.ts";

const B = 3840;
// variants: OSC1=wt|pm (oscillator 1 type), UNISON=1..16, TRACKS=n, WATCH=track index (live probe on that track:
// filter taps run, and the probe is read every 10 calls ≈ the worklet's 30 ms status, timed apart as `probe`),
// AB=1 with WATCH: watched and unwatched alternate per second on one page (`extra` = watched / unwatched − 1)
const OSC1 = process.env.OSC1 === "pm" ? 0.25 : 0.75;
const UNISON = (Number(process.env.UNISON ?? 4) - 1) / 15;
const NT = Number(process.env.TRACKS ?? 12);
// 12 instrument tracks: wavetable + saw with 4-voice unison, 6-note chords retriggered every beat, ladder + SVF
// filters, LFO → cutoff, Echo / Chorus inserts, a reverb bus and a compressor on the master.
const track = (i: number) => ({
  id: `t${i}`, name: `Dense ${i}`, volume: 0.15, mute: false, receives: [],
  devices: [
    { kind: 64, values: { "osc1.type": OSC1, "osc2.type": 0.5, unison: UNISON, capacity: 47 / 63, "filt1.type": 0.625, "filt1.cutoff": 0.5, "filt2.type": 0.125,
      "lfo1.depth": 0.5, "mod1.source": 5 / 9, "mod1.target": 52 / 255, "mod1.amount": 0.4, "env1.release": 0.3 } },
    i % 2 ? { kind: 68, values: { mode: 0, feedback: 0.4, dry_wet: 0.3 } } : { kind: 69, values: { mode: 0, rate: 0.3, depth: 0.5, gain: 0.5 } },
  ],
  placements: [{ clip: "chords", start: 0, length: 8 * B, loop: true }], lanes: [],
});
const song = {
  format: "song/1", rate: 44100, tempo: 140, signature: [4, 4], ticks_per_beat: 960, length: 8 * B,
  clips: [{ id: "chords", length: B, notes: [0, 960, 1920, 2880].flatMap((t) => [48, 52, 55, 59, 62, 67].map((p) => [t, 900, p, 100])) }],
  tracks: [...Array(NT)].map((_, i) => track(i + 1)).concat([{ id: "verb", name: "Verb", volume: 0.5, mute: false, receives: [...Array(NT)].map((_, i) => ({ from: `t${i + 1}`, channel: "main", volume: 0.3 })), devices: [{ kind: 71, values: { mode: 0, dry_wet: 1 } }], placements: [], lanes: [] } as any]),
  master: { id: "master", name: "Master", volume: 1, mute: false, receives: [...Array(NT)].map((_, i) => ({ from: `t${i + 1}`, channel: "main", volume: 1 })).concat([{ from: "verb", channel: "main", volume: 1 }]), devices: [{ kind: 72, values: { mode: 0 } }], placements: [], lanes: [] },
};

if (process.env.SONG) Object.assign(song, JSON.parse(await Bun.file(process.env.SONG).text()));
if (process.env.WRITE) { await Bun.write(process.env.WRITE, JSON.stringify(song)); process.exit(0); }
await buildDaw({ wasm: false });
const s = await startServer({ port: 0, build: false, dawDist: DIST, headless: false });
const b = await chromium.launch({ channel: process.env.CHROME_CHANNEL ?? "chrome" });
const p = await b.newPage();
await p.goto(`${s.url}daw/`);
await p.waitForFunction(() => (window as any).DAW?.engine);
const WATCH = Number(process.env.WATCH ?? -1);
const AB = !!process.env.AB;
const r = await p.evaluate(([json, watch, AB]) => {
  const E = (window as any).DAW.engine, x = E.x;
  const ex = E.exportBlob(json); if (!ex.blob) return { error: JSON.stringify(ex.errors) };
  const ptr = x.alloc(ex.blob.length); new Uint8Array(x.memory.buffer, ptr, ex.blob.length).set(ex.blob);
  if (!x.load(ptr, ex.blob.length)) return { error: "load failed" };
  x.probe_watch?.(watch);  // absent in builds before add-device-live-view (A/B)
  x.play(0);
  let probe = 0, reads = 0;
  const calls = AB ? 8 * 4725 : Math.floor((30 * 44100) / 128), times = new Float64Array(calls);
  for (let i = 0; i < 200; i++) x.render(128); // warm-up
  // AB: watched and unwatched alternate in blocks on the same page, so machine load hits both alike. The 8-bar loop is
  // 4725 calls = 27 blocks of 175 (odd), so each block's music is measured both ways over an even number of loops.
  const ab = { on: 0, off: 0, nOn: 0, nOff: 0 }, BLK = 175;
  for (let i = 0; i < calls; i++) {
    const on = watch >= 0 && (!AB || Math.floor(i / BLK) % 2 === 0);
    if (AB && i % BLK === 0) x.probe_watch(on ? watch : -1);
    const t0 = performance.now(); x.render(128); times[i] = performance.now() - t0;
    if (on) { ab.on += times[i]; ab.nOn++; } else { ab.off += times[i]; ab.nOff++; }
    if (on && i % 10 === 0) { const t1 = performance.now(); const n = x.probe_len(); new Float32Array(x.memory.buffer, x.probe_ptr(), n).slice(); probe += performance.now() - t1; reads++; }
  }
  const sorted = [...times].sort((a, b) => a - b), q = (f: number) => sorted[Math.min(sorted.length - 1, Math.floor(f * sorted.length))];
  return { calls, mean: times.reduce((a, b) => a + b, 0) / calls, p50: q(0.5), p99: q(0.99), p999: q(0.999), max: sorted[sorted.length - 1], over: times.filter((t) => t > 2.9).length, res: Math.min(...[...new Set(sorted)].slice(1).map((v, i, a) => (i ? v - a[i - 1] : Infinity))), probe: reads ? probe / reads : 0,
    ...(AB ? { watched: ab.on / ab.nOn, unwatched: ab.off / ab.nOff, extra: ab.on / ab.nOn / (ab.off / ab.nOff) - 1 } : {}) };
}, [JSON.stringify(song), WATCH, AB] as const);
console.log(`osc1=${process.env.OSC1 ?? "wt"} unison=${process.env.UNISON ?? 4} tracks=${NT} watch=${WATCH}`);
console.log(JSON.stringify(r, (k, v) => (typeof v === "number" ? +v.toFixed(4) : v)));
console.log(`budget per 128 frames at 44.1 kHz: 2.902 ms · max ${((r as any).max / 2.902 * 100).toFixed(0)}% · p99 ${((r as any).p99 / 2.902 * 100).toFixed(0)}%`);
await b.close();
await s.stop();
