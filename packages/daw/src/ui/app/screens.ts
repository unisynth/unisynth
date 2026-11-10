// Device slot screens (change daw-solid-ts 9.1, step 7): small SVG drawings of an oscillator, filter, LFO or envelope,
// redrawn by the panel through `_draw` from the values shown now (automation at the playhead, else stored).
import type { Device } from "../../model.ts";
import type { Param } from "../../engine.ts";
import { choice, DRUM, engineFrames, engineValue, envCurve, envRate, ip, oscType, pointCycle, TONE, vget } from "../model/params.ts";
import { E } from "./session.ts";
import { curVal } from "./automation.ts";
import { css } from "./theme.ts";

// Slot screens (D4): an SVG in a 100×40 box (stretched), redrawn from the current values through the group's
// `_redraw` (knobs, automation, selects). Drawings, not measurements: shapes follow the parameters' meaning.
const SVGNS = "http://www.w3.org/2000/svg";
export type Screen = SVGSVGElement & { _draw?: () => void; _geo?: unknown };
function scrSvg(draw: () => string): Screen {
  const svg: Screen = document.createElementNS(SVGNS, "svg"); svg.setAttribute("class", "scr"); svg.setAttribute("viewBox", "0 0 100 40");
  svg.setAttribute("preserveAspectRatio", "none"); svg.setAttribute("aria-hidden", "true");
  svg._draw = () => { svg.innerHTML = draw(); }; return svg;
}
const scrPath = (f: (x: number) => number, n = 160): string => "M" + Array.from({ length: n + 1 }, (_, k) => `${(k * 100 / n).toFixed(2)} ${Math.max(.5, Math.min(39.5, f(k / n))).toFixed(2)}`).join(" L");
const scrLine = (d: string, fill?: boolean): string => (fill ? `<path class="fl" d="${d} L100 40 L0 40 Z"/>` : "") + `<path class="ln" d="${d}"/>`;
const scrMid = `<line class="mk0" x1="0" y1="20" x2="100" y2="20"/>`;
/** Oscillator waveform (phase mod with feedback, saw/pulse, wavetable, points), or a PAD table's partials. */
export function oscView(d: Device, i: number): Screen {
  return scrSvg(() => {
    const ty = oscType(d, i), v = (n: string): number => curVal(d, `osc${i}.${n}`), TAU = 2 * Math.PI;
    if (ty === 13) {
      const n = 1 + Math.min(3, Math.floor(Math.max(0, Math.min(1, v("points"))) * 3 + .5));
      const { wave, pts } = pointCycle(n, [0, 2, 4, 6].map((k) => v(`s${k}`)), [1, 3, 5, 7].map((k) => v(`s${k}`)),
        ["mode1", "mode2", "mode3", "mode4", "mode_last"].map((f) => choice(v(f), 3)));
      const y = (w: number): number => 20 - 17 * Math.max(-1.15, Math.min(1.15, w));
      return scrMid + scrLine(scrPath((x) => y(wave[Math.round(x * 256)]!), 256)) +
        pts.map(([x, w]) => `<circle class="mk" cx="${(x * 100).toFixed(2)}" cy="${y(w).toFixed(2)}" r="1.4"/>`).join("");
    }
    if (ty === 14) return padSketch(v("s0"), v("s1"), v("s2"), v("s3"), choice(v("s4"), 4));
    if (ty === 15) {
      // Elevated's pair (unisynth `osc::pair_wave`): two copies at 2 − d and d turns per cycle, shifted, folded to ±1
      const wave = choice(v("s0"), 3), sh = v("s1"), dd = 2 * v("s2"), sign = choice(v("s3"), 2) ? -1 : 1;
      const one = (t: number): number => { const x = 2 * (t - Math.round(t)); return wave === 0 ? Math.sin(Math.PI * x) : wave === 1 ? (x > 0 ? 1 : -1) : x; };
      return scrMid + scrLine(scrPath((x) => 20 - 8 * sign * (one(2 * x * (2 - dd) + sh) + one(2 * x * dd + sh))));
    }
    if (ty === TONE || ty === DRUM) {
      // the engine's own first frames (add-fudge-softsynth): four carrier periods of a tone, a drum to its envelope's end
      const sh = [0, 1, 2, 3, 4, 5, 6, 7, 8].map((k) => v(`s${k}`));
      const slope = engineValue(DRUM, 7, sh[7]!), start = engineValue(DRUM, 6, sh[6]!);
      const n = ty === TONE ? Math.min(4096, Math.ceil(4 * engineValue(TONE, 0, sh[0]!) / Math.max(1e-3, engineValue(TONE, 7, sh[7]!))))
        : Math.min(4096, slope > 0 ? Math.ceil(start / slope) : 1024);
      const w = engineFrames(ty, sh, Math.max(2, n));
      return scrMid + scrLine(scrPath((x) => 20 - 18 * w[Math.min(w.length - 1, Math.round(x * (w.length - 1)))]!, 200));
    }
    let f: (x: number) => number;
    if (ty === 2) { const pulse = choice(v("s0"), 2), pw = .05 + .9 * v("s1"); f = (x) => { const ph = (x * 2) % 1; return 20 - 15 * (pulse ? (ph < pw ? 1 : -1) : 2 * ph - 1); }; }
    else if (ty === 3) { const hn = 1 + Math.round(v("s1") * 15), dm = 1 + v("s2") * 3; let pk = 0; const g = (x: number): number => { let y = 0; for (let k = 1; k <= hn; k++) y += Math.sin(TAU * 2 * k * x) / k ** dm; return y; };
      for (let k = 0; k <= 64; k++) pk = Math.max(pk, Math.abs(g(k / 128))); f = (x) => 20 - 15 * g(x) / (pk || 1); }
    else { const fb = v("feedback") * 2.5; f = (x) => 20 - 15 * Math.sin(TAU * 2 * x + fb * Math.sin(TAU * 2 * x)); }
    return scrMid + scrLine(scrPath(f));
  });
}
/**
 * A PAD table's first 16 partials on a harmonic axis (unisynth `pad`): amplitude 1/(1 + h(1 − damp)) (twice for even
 * h), width (2^(256·bandwidth/1200) − 1)·h^(1 + 19·scale²) harmonics and the profile's shape (its name is the profile
 * knob's text). A drawing: heights are each partial's peak.
 */
function padSketch(bw: number, scale: number, damp: number, harmonics: number, profile: number): string {
  const H = 16 + Math.round(harmonics * 255 / 4), base = 2 ** (256 * bw / 1200) - 1, ex = 1 + 19 * scale * scale, X = 17;
  const shape = (u: number): number => profile === 0 ? (Math.abs(u) <= .1 ? 1 : 0) : profile === 1 ? (Math.abs(u) > .4 && Math.abs(u) <= .5 ? 1 : 0)
    : profile === 2 ? (Math.abs(u) <= 5 ? 1 : 0) : Math.exp(-u * u);
  const amp = (h: number): number => (h % 2 ? 1 : 2) / (1 + h * (1 - damp));
  let pk = 0; for (let h = 1; h <= Math.min(H, X); h++) pk = Math.max(pk, amp(h));
  const f = (x: number): number => {
    const hx = x * X; let y = 0;
    for (let h = 1; h <= Math.min(H, X); h++) { const w = Math.max(base * h ** ex / 2, 1e-3); y = Math.max(y, amp(h) / pk * shape((hx - h) / w)); }
    return 38 - 33 * y;
  };
  return scrLine(scrPath(f, 400), true);
}
// filter response on a 20 Hz…20 kHz log axis (x = 100·log10(f/20)/3), 0 dB near the top, −60 dB at the bottom
/** Filter screens: dB → y (also the live overlays). */
export const yDb = (db: number): number => 38 - Math.max(0, Math.min(36, (db + 60) * .55));
/** Filter response for the current type, cutoff and resonance. */
export function filtView(d: Device, i: number): Screen {
  return scrSvg(() => {
    const p = ip(`filt${i}.cutoff`), hz = E.physical(d.kind, 0, p.index, curVal(d, p.name)), cx = Math.max(0, Math.min(100, Math.log10(hz / 20) / .03));
    const name = ((p0: Param) => p0.options[choice(vget(d, p0), p0.options.length)] || "")(ip(`filt${i}.type`)).toLowerCase();
    const q = .6 + curVal(d, `filt${i}.resonance`) * 6, poles = /ladder/.test(name) ? 2 : 1;
    const h = (x: number): number => { let r = 10 ** ((x - cx) * .03); if (/high/.test(name)) r = 1 / r;
      const den = Math.sqrt((1 - r * r) ** 2 + (r / q) ** 2), lp = 1 / den;
      return /band/.test(name) ? (r / q) / den : /notch/.test(name) ? Math.abs(1 - r * r) / den : lp ** poles; };
    return scrLine(scrPath(x => yDb(20 * Math.log10(h(x * 100) + 1e-9))), true) + `<line class="mk" x1="${cx.toFixed(2)}" y1="0" x2="${cx.toFixed(2)}" y2="40"/>`;
  });
}
/** LFO shapes by choice index (sine, saw up, saw down, square, sample & hold). */
export const LFO_SHAPES: ((x: number) => number)[] = [(x) => Math.sin(2 * Math.PI * x), (x) => 2 * x - 1, (x) => 1 - 2 * x, (x) => x < .5 ? 1 : -1, (x) => Math.sin(Math.floor(x * 8) * 2.7 + 1)];
export function lfoView(d: Device, i: number): Screen {
  return scrSvg(() => { const f = LFO_SHAPES[choice(curVal(d, `lfo${i}.shape`), 5)], dep = .25 + .75 * curVal(d, `lfo${i}.depth`);
    return scrMid + scrLine(scrPath(x => 20 - 15 * dep * f(x), 200)); });
}
// Envelope curve as the engine plays it (crates/unisynth/src/env.rs): WaveSabre mode in closed form (linear attack,
// quadratic decay and release, times `p²·5000 + 1` ms), Tunefish mode integrated per 512-frame block. A note is held
// for a while after the decay, then released. Redrawn from the knobs (`_draw`), so it follows edits and automation.
// Tunefish-mode stage times in seconds (same integration as envCurve): attack 0 → 1, decay 1 → sustain (with the
// current sustain and slope), release from full level to silence. Infinity when the stage never ends.
/** Envelope curve with the note-off mark and its total time. */
export function envView(d: Device, i: number): Screen {
  const W = 196, H = 54, NS = "http://www.w3.org/2000/svg";
  const svg: Screen = document.createElementNS(NS, "svg"); svg.setAttribute("class", "env"); svg.setAttribute("viewBox", `0 0 ${W} ${H}`); svg.setAttribute("aria-hidden", "true");
  svg._draw = () => {
    const v = (n: string): number => curVal(d, `env${i}.${n}`), tf = envRate(v("law"));
    const { pts, off } = envCurve(tf, v("attack"), v("decay"), v("sustain"), v("release"), v("curve_slope"));
    const tot = Math.max(pts[pts.length - 1][0], 1e-3), x = (t1: number): number => 4 + (t1 / tot) * (W - 8), y = (q: number): number => H - 10 - q * (H - 14);
    svg._geo = { x, y, off, tot };  // the live dots use the curve's own time axis
    const step = Math.max(1, Math.ceil(pts.length / 200)), line = pts.filter((_, k) => k % step === 0 || k === pts.length - 1).map(([t1, q]) => x(t1).toFixed(1) + "," + y(q).toFixed(1)).join(" ");
    const fmt = (s: number): string => s >= 1 ? s.toFixed(2) + " s" : Math.round(s * 1000) + " ms";
    svg.innerHTML = `<line x1="${x(off)}" y1="2" x2="${x(off)}" y2="${H - 10}" stroke="${css("--line")}" stroke-dasharray="2 2"/>
      <polyline fill="currentColor" fill-opacity=".14" stroke="currentColor" stroke-width="1.5" points="${x(0)},${y(0)} ${line} ${x(tot)},${y(0)}"/>
      <text x="${x(off) > W - 40 ? x(off) - 2 : x(off) + 2}" y="9" font-size="8" text-anchor="${x(off) > W - 40 ? "end" : "start"}" fill="${css("--dim")}">note off</text>
      <text x="${W - 4}" y="${H - 1}" font-size="8" text-anchor="end" fill="${css("--dim")}">${fmt(tot)}${tf ? " · rate law" : ""}</text>`;
  };
  return svg;
}
