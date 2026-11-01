// Live per-voice displays (change add-device-live-view D7; ported in daw-solid-ts 6.4). The engine's probe of the
// watched (selected) track's instrument arrives with each ~30 ms status; an animation frame redraws only the overlay
// groups (`g.live`) of the registered screens (D4: imperative SVG strings, no per-voice components). Each voice keeps
// one colour (by its note-on id) in every screen and a chip in the instrument header (`chips`); at most 16 voices are
// drawn, the loudest; markers at one spot stack. Filters: input spectrum grey, output in the block colour, a tick per
// voice at its modulated cutoff. The frame loop runs only while the Device tab shows.
import { createEffect, createRoot, createSignal, onCleanup } from "solid-js";
import { deviceShown } from "../state/device.ts";
import type { Device } from "./Device.tsx";

export interface LiveHost {
  /** The Live toggle. */
  on(): boolean;
  /** The watched track's instrument. */
  inst(): Device | null;
  curVal(d: Device, name: string): number;
  choice(v: number, n: number): number;
  lfoShape(k: number): (x: number) => number;
  /** Filter screens' dB → y. */
  yDb(db: number): number;
  noteName(n: number): string;
  rate(): number;
}
type Voice = { id: number; note: number; amp: number; rel: boolean; age: number; env: [number, number][]; lfo: number[]; smp: number[]; cut: number[] };
type Probe = { voices: Voice[]; taps: ([Float32Array, Float32Array] | undefined)[]; spec?: ([Float64Array, Float64Array] | undefined)[] };
type Screen = SVGSVGElement & { _geo?: { x(t: number): number; y(q: number): number; off: number; tot: number } };

const VCOL = ["#ffd166", "#4cc9f0", "#f72585", "#80ed99", "#ff8c42", "#b8a1ff", "#f4f1de", "#00f5d4", "#ff70a6", "#70d6ff", "#e9ff70", "#ff9770", "#c77dff", "#90f1ef", "#ffd6e0", "#a3e635"];
const MAX_DRAWN = 16, VREC = 22;
let H!: LiveHost;
const vcol = new Map<number, number>();  // voice id → palette index
const relAt = new Map<number, number>(); // voice id → age at which it was first seen released
let liveP: Probe | null = null;
let dirty = false;
const cost = { probe: 0, nProbe: 0, draw: 0, nDraw: 0, voices: 0 };  // ms spent (DAW.debug.live)
/** Screens with an overlay: element → `<group><slot>` (osc1, filt2, env1, lfo1, smp3). */
const screens = new Map<Screen, string>();

export type Chip = { id: number; label: string; col: string; rel: boolean };
/** Header chips of the drawn voices, plus how many more sound. */
export const [chips, setChips] = createSignal<{ list: Chip[]; more: number }>({ list: [], more: 0 });

export function initLive(h: LiveHost): void { H = h; }
/** A screen draws per-voice overlays while mounted (returns the unregister). */
export function registerLive(el: Element, key: string): () => void {
  screens.set(el as Screen, key); dirty = true;
  return () => { screens.delete(el as Screen); };
}
/** Redraw at the next frame (a screen redrew and lost its overlay, or the view went off). */
export function markLiveDirty(): void { dirty = true; }
/** The watch went off: forget the last probe. */
export function liveClear(): void { liveP = null; dirty = true; }
export function liveStats(): { probeMs: number; drawMs: number; maxVoices: number } {
  return { probeMs: cost.probe / Math.max(1, cost.nProbe), drawMs: cost.draw / Math.max(1, cost.nDraw), maxVoices: cost.voices };
}

function parseProbe(p: Float32Array | null | undefined): Probe | null {
  if (!p || p[0] !== 1) return null;
  const nv = p[1]!, nf = p[2]!, L = p[3]!, voices: Voice[] = [], taps: Probe["taps"] = [];
  if (p.length < 4 + nv * VREC + nf) return null;  // layout mismatch (VREC = synth-wasm Host::probe's voice record)
  let o = 4;
  for (let v = 0; v < nv; v++, o += VREC) {
    const r = p.subarray(o, o + VREC);
    voices.push({ id: r[0]!, note: r[1]!, amp: r[2]!, rel: r[3]! > 0, age: r[4]!, env: [0, 1, 2, 3].map((k) => [r[5 + 2 * k]!, r[6 + 2 * k]!] as [number, number]),
      lfo: [r[13]!, r[14]!], smp: [r[15]!, r[16]!, r[17]!], cut: [r[18]!, r[19]!, r[20]!, r[21]!] });
  }
  for (let f = 0; f < nf && o < p.length; f++) { if (p[o++]) { if (o + 2 * L > p.length) return null; taps[f] = [p.subarray(o, o + L), p.subarray(o + L, o + 2 * L)]; o += 2 * L; } }
  return { voices, taps };
}

// 2048-point Hann FFT of the taps, folded onto the screens' log axis (x = 100·log10(f/20)/3), with a short peak hold
const FN = 2048, FBINS = 140, fwin = Float64Array.from({ length: FN }, (_, i) => 0.5 - 0.5 * Math.cos((2 * Math.PI * i) / (FN - 1)));
const frev = Uint16Array.from({ length: FN }, (_, i) => { let r = 0; for (let b = 0, x = i; b < 11; b++, x >>= 1) r = (r << 1) | (x & 1); return r; });
const fre = new Float64Array(FN), fim = new Float64Array(FN);
function spectrumDb(x: Float32Array): Float64Array {
  for (let i = 0; i < FN; i++) { fre[frev[i]!] = x[i]! * fwin[i]!; fim[frev[i]!] = 0; }
  for (let len = 2; len <= FN; len <<= 1) {
    const ang = (-2 * Math.PI) / len, wr = Math.cos(ang), wi = Math.sin(ang);
    for (let i = 0; i < FN; i += len) {
      let cr = 1, ci = 0;
      for (let k = 0; k < len / 2; k++) {
        const a = i + k, b = a + len / 2, tr = fre[b]! * cr - fim[b]! * ci, ti = fre[b]! * ci + fim[b]! * cr;
        fre[b] = fre[a]! - tr; fim[b] = fim[a]! - ti; fre[a] = fre[a]! + tr; fim[a] = fim[a]! + ti;
        const nr = cr * wr - ci * wi; ci = cr * wi + ci * wr; cr = nr;
      }
    }
  }
  const out = new Float64Array(FBINS + 1), rate = H.rate() || 44100;
  for (let b = 0; b <= FBINS; b++) {
    const f0 = 20 * 10 ** (((b - 0.5) / FBINS) * 3), f1 = 20 * 10 ** (((b + 0.5) / FBINS) * 3);
    const k0 = Math.max(1, Math.floor((f0 / rate) * FN)), k1 = Math.min(FN / 2 - 1, Math.max(k0, Math.ceil((f1 / rate) * FN)));
    let m = 0; for (let k = k0; k <= k1; k++) m = Math.max(m, Math.hypot(fre[k]!, fim[k]!));
    out[b] = 20 * Math.log10(m / (FN / 4) + 1e-6);
  }
  return out;
}
const specHold = new Map<string, Float64Array>();  // `${slot}:in|out` → dB bins with peak hold
function holdSpec(key: string, db: Float64Array): Float64Array {
  const h = specHold.get(key); if (h) for (let b = 0; b < db.length; b++) db[b] = Math.max(db[b]!, h[b]! - 2.5);
  specHold.set(key, db); return db;
}

/** Each engine status: parse the probe (if the view is on), keep voice colours, compute the filter spectra. */
export function liveProbe(p: Float32Array | null | undefined): void {
  const t0 = performance.now();
  try {
    const pr = H.on() ? parseProbe(p) : null;  // a status sent before the view went off
    if (!pr && !liveP) return;
    liveP = pr; dirty = true;
    if (!pr) return;
    const ids = new Set(pr.voices.map((v) => v.id));
    for (const id of [...vcol.keys()]) if (!ids.has(id)) { vcol.delete(id); relAt.delete(id); }
    for (const v of pr.voices) {
      // a free palette entry; with all 16 taken (more voices than are drawn) the voice shares one, starting at its id
      if (!vcol.has(v.id)) { const used = new Set(vcol.values()); let c = 0; while (used.has(c) && c < VCOL.length) c++; vcol.set(v.id, c < VCOL.length ? c : v.id % VCOL.length); }
      if (v.rel && !relAt.has(v.id)) relAt.set(v.id, v.age);
    }
    pr.spec = pr.taps.map((t, f) => t && [holdSpec(`${f}:in`, spectrumDb(t[0])), holdSpec(`${f}:out`, spectrumDb(t[1]))] as [Float64Array, Float64Array]);
  } finally { cost.probe += performance.now() - t0; cost.nProbe++; }
}

const vmark = (x: number, y: number, c: string, a: number, w = 6) => `<line class="vm" x1="${x.toFixed(2)}" y1="${y.toFixed(2)}" x2="${x.toFixed(2)}" y2="${y.toFixed(2)}" stroke="${c}" stroke-width="${w}" stroke-opacity="${a.toFixed(2)}"/>`;
function beads(h: typeof vmark) {
  const seen: [number, number][] = [];
  return (x: number, y: number, c: string, a: number, dy: number, top: number) => {
    const k = seen.filter((b) => Math.abs(b[0] - x) < dy * 0.6 && Math.abs(b[1] - y) < dy).length; seen.push([x, y]);
    return h(x, y + (y < top ? 1 : -1) * k * dy, c, a);
  };
}

function draw(): void {
  if (!dirty) return; dirty = false;
  const t0 = performance.now();
  try { draw1(); } finally { cost.draw += performance.now() - t0; cost.nDraw++; cost.voices = Math.max(cost.voices, liveP?.voices.length || 0); }
}
function draw1(): void {
  for (const svg of screens.keys()) svg.querySelector(":scope > g.live")?.remove();
  const vs = (liveP?.voices || []).slice().sort((a, b) => b.amp - a.amp), shown = vs.slice(0, MAX_DRAWN);
  setChips({ list: shown.map((v) => ({ id: v.id, label: `${H.noteName(v.note % 12)}${Math.floor(v.note / 12) - 1}`, col: VCOL[vcol.get(v.id)!]!, rel: v.rel })), more: Math.max(0, vs.length - MAX_DRAWN) });
  if (shown.length) {
    const d = H.inst(), alpha = (v: Voice) => 0.25 + 0.75 * Math.min(1, v.amp / Math.max(1e-6, shown[0]!.amp)), col = (v: Voice) => VCOL[vcol.get(v.id)!]!;
    for (const [svg, key] of screens) {
      if (!svg.isConnected || svg.closest(".closed")) continue;
      const m = key.match(/^([a-z]+)(\d)$/)!, g = m[1], i = +m[2]!;
      let h = "";
      if (g === "smp") for (const v of shown) {
        const pos = v.smp[i - 1]!; if (pos < 0) continue; const x = Math.min(100, pos * 100);
        h += `<line class="vm" x1="${x}" y1="0" x2="${x}" y2="40" stroke="${col(v)}" stroke-width="1.5" stroke-opacity="${alpha(v).toFixed(2)}"/>`;
      }
      if (g === "env" && svg._geo) {
        const { x, y, off, tot } = svg._geo, bead = beads(vmark), vb = svg.viewBox.baseVal;
        shown.forEach((v, k) => {
          const [lvl] = v.env[i - 1]!, ra = relAt.get(v.id), t = ra == null ? Math.min(v.age, off) : Math.min(tot, off + (v.age - ra)), c = col(v);
          h += bead(x(t), y(lvl), c, v.rel ? 0.55 : 1, 5, vb.height / 2) + `<line class="vm" x1="${vb.width - 3 - (k % 4) * 1.5}" y1="${y(0)}" x2="${vb.width - 3 - (k % 4) * 1.5}" y2="${y(lvl)}" stroke="${c}" stroke-width="1.5" stroke-opacity=".8"/>`;
        });
      }
      if (g === "lfo" && d) {
        const f = H.lfoShape(H.choice(H.curVal(d, `lfo${i}.shape`), 5)), dep = 0.25 + 0.75 * H.curVal(d, `lfo${i}.depth`), bead = beads(vmark);
        for (const v of shown) { const ph = ((v.lfo[i - 1]! % 1) + 1) % 1; h += bead(ph * 100, 20 - 15 * dep * f(ph), col(v), alpha(v), 4.5, 20); }
      }
      if (g === "filt") {
        const sp = liveP!.spec?.[i - 1];
        if (sp) {
          const path = (db: Float64Array) => "M" + Array.from(db, (v, b) => `${((b / FBINS) * 100).toFixed(2)} ${H.yDb(v).toFixed(2)}`).join(" L");
          const pin = path(sp[0]); h += `<path class="sin" d="${pin} L100 40 L0 40 Z"/><path class="sinl" d="${pin}"/><path class="sout" d="${path(sp[1])}"/>`;
        }
        for (const v of shown) {
          const hz = v.cut[i - 1]!; if (hz < 0) continue; const x = Math.max(0, Math.min(100, Math.log10(Math.max(hz, 20) / 20) / 0.03));
          h += `<line class="vm ct" x1="${x.toFixed(2)}" y1="0" x2="${x.toFixed(2)}" y2="5" stroke="${col(v)}" stroke-width="2" stroke-opacity="${alpha(v).toFixed(2)}"/>`;
        }
      }
      if (h) svg.insertAdjacentHTML("beforeend", `<g class="live">${h}</g>`);
    }
  }
  // static filter curves step back while spectra are drawn
  for (const [svg, key] of screens) if (key.startsWith("filt") && svg.classList.contains("scr")) svg.classList.toggle("playing", !!svg.querySelector("g.live path.sout"));
}

// the frame loop, only while the Device tab shows (the panel and its screens exist only then)
createRoot(() => createEffect(() => {
  if (!deviceShown()) return;
  let id = 0;
  const frame = () => { draw(); id = requestAnimationFrame(frame); };
  id = requestAnimationFrame(frame);
  onCleanup(() => cancelAnimationFrame(id));
}));
