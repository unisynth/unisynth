// Device parameters from the engine's tables (change daw-solid-ts 9.1, step 1; design D8 of add-synth-daw): lookup
// by device kind and mode, the instrument's parameters by name, slot state, envelope maths and the value text a
// knob shows. Set once by `initParams` (main tables + the engine's physical-value law); pure afterwards.
// Reads of `d` go through `raw` where hot (store proxies, ui/state/song.ts); `fmtVal` reads through the proxy on
// purpose so knob text tracks law/sustain/slope/osc type.
import type { Device } from "../../model.ts";
import type { Param, Table } from "../../engine.ts";
import { raw } from "../state/song.ts";

/** Slot counts of the instrument's groups (unisynth). */
export const SLOTS = { osc: 3, filt: 4, env: 4, lfo: 2, mod: 12 } as const;
export type SlotGroup = keyof typeof SLOTS;

let TABLES: Table[] = [];
let physical: (kind: number, mode: number, index: number, p: number) => number = () => 0;
let INST_T: Table | undefined;
let byName = new Map<string, Param>();

export function initParams(tables: Table[], phys: typeof physical): void {
  TABLES = tables; physical = phys;
  INST_T = tables.find((t) => t.kind === 64);
  byName = new Map((INST_T?.params ?? []).map((p) => [p.name, p]));
  // an effect kind's stored mode positions (its mode parameter's options) and the mode at each: the engine expanded
  // some mode choices (add-elevated-import), so a stored position is not the mode id; each table's mode default is its
  // own stored value
  BY_POS = new Map();
  for (const t of tables) {
    const mp = t.kind === 64 ? undefined : t.params.find((p) => p.name === "mode");
    if (!mp) continue;
    const n = Math.max(1, mp.options.length), e = BY_POS.get(t.kind) ?? { n, mode: Array<number>(n).fill(-1) };
    e.mode[choice(mp.default, n)] = t.mode; BY_POS.set(t.kind, e);
  }
}
/** Per effect kind: stored mode positions and the mode id at each (−1 = a free position). */
let BY_POS = new Map<number, { n: number; mode: number[] }>();

/** Option index of a 0–1 value over `n` options. */
export const choice = (p: number, n: number): number => Math.min(n - 1, Math.max(0, Math.floor(p * (n - 1) + .5)));
export const nModes = (k: number): number => TABLES.filter((t) => t.kind === k).length;
/** Mode of an effect (0 for the instrument); read-only and hot: reads the raw device. A free position reads as mode 0. */
export const modeOf = (d: Device): number => {
  const r = raw(d); if (r.kind === 64) return 0;
  const e = BY_POS.get(r.kind); if (!e) return 0;
  return Math.max(0, e.mode[choice(r.values.mode ?? 0, e.n)] ?? 0);
};
/** The stored value of effect kind `k`'s mode `mode` (the value its table's mode parameter defaults to). */
export const modeValue = (k: number, mode: number): number => TABLES.find((t) => t.kind === k && t.mode === mode)?.params.find((p) => p.name === "mode")?.default ?? 0;
export const tableOf = (d: Device): Table | undefined => { const k = raw(d).kind, m = modeOf(d); return TABLES.find((t) => t.kind === k && t.mode === m); };
export const paramOf = (d: Device, name: string): Param | undefined => tableOf(d)?.params.find((p) => p.name === name);
export const devName = (d: Device): string => d.kind === 64 ? (d.preset || "Unisynth") : tableOf(d)?.label ?? `kind ${d.kind}`;
/** Automation targets: every parameter of a device that plays live (not structural, raw or a PAD table input). */
export const autoParams = (d: Device): Param[] => (tableOf(d)?.params || []).filter((p) => !loadTime(d, p) && !p.raw);
/** Effects that can be added (every table but the instrument's). */
export const effectChoices = (): Table[] => TABLES.filter((tb) => tb.kind !== 64);

/** The instrument's parameter table. */
export const INST = (): Table => INST_T!;
/** An instrument parameter by name. */
export const ip = (name: string): Param => byName.get(name)!;
export const vget = (d: Device, p: Param): number => d.values[p.name] ?? p.default;
/** Option name of an instrument choice parameter. */
export function optName(d: Device, name: string): string | undefined { const p = ip(name), o = p.options || []; return o[choice(vget(d, p), o.length)]; }
/** An envelope `law` value plays the rate law. */
export function envRate(v: number): boolean { const o = ip("env1.law").options; return o[choice(v, o.length)] === "Rate"; }
/** The stored value of a choice parameter's option `label` (from the engine's table, so no encoding is assumed here). */
export function optValue(name: string, label: string): number { const o = ip(name).options || [], i = o.indexOf(label); return i <= 0 || o.length < 2 ? 0 : i / (o.length - 1); }
/** The label of a free position of an expanded choice (the engine's tables; never offered). */
export const GAP = "—";
/**
 * Oscillator kind of slot `i`, by its option label (not the engine's stored position, which add-tf3-import expanded):
 * 0 Off … 4 Sampler, 5–11 the naive generators, 12 Clocked, 13 Points, 14 PAD table, 15 Pair (add-elevated-import).
 */
export function oscType(d: Device, i: number): number {
  return Math.max(0, ["Off", "Phase mod", "Saw/pulse", "Wavetable", "Sampler", "Naive saw/tri", "Naive pulse", "Sine", "Coloured noise", "FM",
    "Aux in A", "Aux in B", "Clocked", "Points", "PAD table", "Pair"].indexOf(optName(d, `osc${i}.type`) ?? ""));
}
/** PAD table profiles (`osc<i>.s4` of a PAD slot, a choice over 4; engine `pad`). */
export const PAD_PROFILES = ["single", "detune", "spread", "gauss"];
/**
 * Whether `p` takes effect only when the instrument loads (mirrors unisynth `params::is_load_time`): the structural
 * parameters and a PAD slot's table inputs (`osc<i>.s0`–`s4`). The engine rebuilds the device when one changes, so a
 * knob drag on one pushes once, on release, and it is no automation target.
 */
export function loadTime(d: Device, p: Param): boolean {
  if (p.structural) return true;
  const m = /^osc(\d)\.s[0-4]$/.exec(p.name);
  return !!m && raw(d).kind === 64 && oscType(d, +m[1]!) === 14;
}
/** Sampler screens a slot shows: the waveform, plus loop and pitch when looping. */
export const smpShapes = (d: Device, i: number): number[] => [0, ...(choice(vget(d, ip(`osc${i}.s1`)), 3) && choice(vget(d, ip(`osc${i}.s2`)), 2) ? [3, 4] : [])];
const slotKeyCache = new WeakMap<Table, Map<string, string[]>>();
/** Parameter names of one slot (`osc2` → `osc2.*`), cached per instrument table (called per control on a panel mount). */
export const slotKeys = (g: string, i: number): string[] => {
  const T = INST(); let m = slotKeyCache.get(T); if (!m) slotKeyCache.set(T, m = new Map());
  const k = `${g}${i}.`; let r = m.get(k); if (!r) m.set(k, r = T.params.filter((p) => p.name.startsWith(k)).map((p) => p.name)); return r;
};
/** Mod target names (by stored index / 255). */
export const TARGETS = (): string[] => ip("mod1.target").options;
export function modSrcName(d: Device, m: number): string { const p = ip(`mod${m}.source`); return p.options[choice(vget(d, p), p.options.length)] || "None"; }
/** Mod targets an envelope or LFO slot routes to (by its mod rows). */
export function routesOf(d: Device, g: SlotGroup, i: number): string[] {
  const src = `${g === "env" ? "Env" : "LFO"} ${i}`, out: string[] = [];
  for (let m = 1; m <= SLOTS.mod; m++) if (modSrcName(d, m) === src) out.push(TARGETS()[Math.round(vget(d, ip(`mod${m}.target`)) * 255)] || "?");
  return out;
}
export function slotUsed(d: Device, g: SlotGroup, i: number): boolean {
  if (g === "osc") return oscType(d, i) !== 0;
  if (g === "filt") return optName(d, `filt${i}.type`) !== "Off";
  if (g === "mod") return modSrcName(d, i) !== "None";
  if (g === "env" && i === 1) return true;
  return slotKeys(g, i).some((n) => n in d.values && d.values[n] !== ip(n).default) || (g === "env" || g === "lfo") && routesOf(d, g, i).length > 0;
}

/** Classic envelope stage time (ms) of a 0–1 value. */
export function envMs(p: number): number { return p * p * 5000 + 1; }
/** Tunefish rate-law stage time (s); Infinity when it never ends. */
export function tfEnvSeconds(stage: string, x: number, s: number, slope: number): number {
  const B = 512 / 44100, STEP = 0.0005 * 512, cube = (q: number) => q * q * q, MAX = 600 / B;
  if (stage === "attack") return B * (1 / (-Math.log10(Math.max(cube(x), 1e-9) * 0.94) * STEP));
  const rate = Math.log10(Math.max(cube(x), 1e-9) * 0.94) * 0.25 * STEP; if (rate >= 0) return Infinity;
  let v = 1, n = 0;
  if (stage === "decay") { const sus = Math.min(s, 0.99); while (v > sus && n < MAX) { v += rate * ((0.01 + v - sus) / (1 - sus)) ** slope; n++; } }
  else while (v > 1e-5 && n < MAX) { v += rate * Math.max(v, 0) ** slope; n++; }
  return n >= MAX ? Infinity : n * B;
}
/** Envelope shape for the screen: points [s, level] and the note-off time. */
export function envCurve(tf: boolean, a: number, dcy: number, s: number, r: number, slope: number): { pts: [number, number][]; off: number } {
  const pts: [number, number][] = [];
  if (!tf) {
    const A = envMs(a) / 1000, D = envMs(dcy) / 1000, R = envMs(r) / 1000, hold = Math.max(0.05, 0.25 * (A + D));
    pts.push([0, 0], [A, 1]);
    for (let k = 1; k <= 24; k++) { const f = (1 - k / 24) ** 2; pts.push([A + D * k / 24, f + s * (1 - f)]); }
    const off = A + D + hold; pts.push([off, s]);
    for (let k = 1; k <= 24; k++) pts.push([off + R * k / 24, s * (1 - k / 24) ** 2]);
    return { pts, off };
  }
  const B = 512 / 44100, STEP = 0.0005 * 512, cube = (x: number) => x * x * x, sus = Math.min(s, 0.99), MAX = 20 / B;
  let v = 0, n = 0, stage = 0; pts.push([0, 0]);
  const up = -Math.log10(Math.max(cube(a), 1e-9) * 0.94) * STEP, dn = Math.log10(Math.max(cube(dcy), 1e-9) * 0.94) * 0.25 * STEP;
  while (stage < 2 && n < MAX) {
    if (stage === 0) { v += up; if (v >= 1) { v = 1; stage = 1; } }
    else if (v < sus) { v = sus; stage = 2; }
    else { v += dn * ((0.01 + v - sus) / (1 - sus)) ** slope; if (v <= sus) { v = sus; stage = 2; } v = Math.min(v, 1); if (dn >= 0) stage = 2; }
    n++; pts.push([n * B, v]);
  }
  const off = n * B + Math.max(0.05, 0.25 * n * B); pts.push([off, v]);
  const rel = Math.log10(Math.max(cube(r), 1e-9) * 0.94) * 0.25 * STEP;
  for (let k = 1; v > 1e-5 && k < MAX; k++) { v += rel * Math.max(v, 0) ** slope; if (rel >= 0) break; pts.push([off + k * B, Math.max(v, 0)]); }
  return { pts, off };
}

/** Pan text: C, L1–L100, R1–R100. */
export const panText = (p: number): string => p === .5 ? "C" : (p < .5 ? "L" + Math.round((.5 - p) * 200) : "R" + Math.round((p - .5) * 200));
/** Physical value text of a device parameter (laws from the tables; the engine computes the value). */
export function fmtVal(d: Device, p: Param, v: number): string {
  if (p.law === "Index") return p.options[Math.round(v * 255)] ?? String(Math.round(v * 255));
  if (p.options.length) return p.options[choice(v, p.options.length)] ?? String(v);
  if (p.raw) return String(Math.round(v));
  if (/^osc\d\.s[034]$/.test(p.name) && oscType(d, +p.name[3]!) === 4) return (v * 100).toFixed(1) + " %";
  if (/^osc\d\.s4$/.test(p.name) && oscType(d, +p.name[3]!) === 14) return PAD_PROFILES[choice(v, 4)]!;
  const x = physical(d.kind, modeOf(d), p.index, v), law = p.law;
  if (law === "Cutoff" || law === "FixedHz" || law === "LfoRate") return x >= 1000 ? (x / 1000).toFixed(2) + "k" : x.toFixed(x < 10 ? 2 : 0) + " Hz";
  if (law === "Gain") return x <= 0 ? "−∞ dB" : (20 * Math.log10(x)).toFixed(1) + " dB";
  if (law === "Time") return x.toFixed(x < 1 ? 3 : 2) + " s";
  const secs = (t: number): string => t >= 1 ? t.toFixed(2) + " s" : Math.round(t * 1000) + " ms";
  // Elevated's fields (add-elevated-import): linear envelope times, the drop's base and its halving time
  if (law === "LinTime") return secs(x / 1000);
  if (law === "DropHz") return x.toFixed(x < 100 ? 1 : 0) + " Hz";
  if (law === "DropRate") return x >= 1 ? "off" : "½ in " + secs(Math.log(0.5) / Math.log(x) / 44100);
  if (law === "EnvTime") {
    const m = /^env(\d)\.(attack|decay|release)$/.exec(p.name), tf = m && d.kind === 64 && envRate(vget(d, ip(`env${m[1]}.law`)));
    // a linear envelope's times are 5000·p² ms, no 1 ms floor (unisynth `law::lin_ms`)
    if (m && d.kind === 64 && choice(vget(d, ip(`env${m[1]}.linear`)), 2) === 1) return secs(5 * v * v);
    const s = tf ? tfEnvSeconds(m[2]!, v, vget(d, ip(`env${m[1]}.sustain`)), vget(d, ip(`env${m[1]}.curve_slope`))) : envMs(v) / 1000;
    return !isFinite(s) ? (m?.[2] === "decay" ? "holds" : "∞") : s >= 1 ? s.toFixed(2) + " s" : Math.round(s * 1000) + " ms";
  }
  if (law === "Transpose" || law === "Coarse") return (x > 0 ? "+" : "") + Math.round(x) + " st";
  if (law === "Fine") return (x >= 0 ? "+" : "") + x.toFixed(2) + " st";
  if (law === "Octave") return (x > 0 ? "+" : "") + x;
  if (law === "Ratio" || law === "RatioFine") return x.toFixed(3) + "×";
  if (law === "Feedback" || law === "PmOut") return x.toFixed(2);
  if (law === "Pan") return panText(v);
  if (law.startsWith("Count")) return String(x);
  return (v * 100).toFixed(0) + "%";
}
/**
 * A Points slot's cycle as the engine builds it (unisynth `pts::cycle`): the curve runs from (0, 0) through the points
 * (offset = fraction of the span left, value 0..1 → −1..1) to (1, 0); each segment steps, runs linear or follows the
 * natural spline that the original solves one phantom segment past the end, back to (0, 0); the mean is removed.
 * Returns 257 samples and the points.
 */
export function pointCycle(count: number, offset: number[], value: number[], modes: number[]): { wave: number[]; pts: [number, number][] } {
  const n = Math.max(1, Math.min(4, count)), p: [number, number][] = [[0, 0]];
  let last = 0;
  for (let k = 0; k < n; k++) { const x = last + (1 - last) * Math.max(.01, Math.min(.99, offset[k]!)); last = x; p.push([x, value[k]! * 2 - 1]); }
  p.push([1, 0], [0, 0]);
  // the natural spline's tridiagonal sweep over every real segment and the phantom one (widths and rises + 1e-4);
  // built with push and locals (no indexed writes)
  const m = n + 2, dx = Array.from({ length: m }, (_, j) => p[j + 1]![0] - p[j]![0] + 1e-4), dy = Array.from({ length: m }, (_, j) => p[j + 1]![1] - p[j]![1] + 1e-4);
  const r: number[] = [0], z: number[] = [0];
  for (let j = 1; j < m; j++) {
    let rj = (dy[j]! / dx[j]! - dy[j - 1]! / dx[j - 1]!) * 3, zj = (dx[j - 1]! + dx[j]!) * 2;
    if (j >= 2) { rj -= r[j - 1]! * dx[j - 1]! / z[j - 1]!; zj -= dx[j - 1]! * dx[j - 1]! / z[j - 1]!; }
    r.push(rj); z.push(zj);
  }
  const back: number[] = [];
  let next = 0;
  for (let j = m - 1; j >= 1; j--) { next = (r[j]! - dx[j]! * next) / z[j]!; back.unshift(next); }
  const b = [0, ...back, 0];
  const mode = (j: number): number => j <= n ? modes[j - 1]! : modes[4]!;
  const wave = Array.from({ length: 257 }, (_, s1) => {
    const x = s1 / 256; let j = 1; while (x > p[j]![0] && j <= n) j++;
    const k = j - 1, [x0, y0] = p[k]!, [x1, y1] = p[j]!;
    if (mode(j) === 0) return y0;
    if (mode(j) === 1) return y0 + (y1 - y0) * ((x - x0) / (x1 - x0));
    const t = x - x0, c = dy[k]! / dx[k]! - dx[k]! * (b[k + 1]! + 2 * b[k]!) / 3, a = (b[k + 1]! - b[k]!) / (3 * dx[k]!);
    return ((a * t + b[k]!) * t + c) * t + y0;
  });
  const mean = wave.reduce((q, w) => q + w, 0) / 256;
  return { wave: wave.map((w) => w - mean), pts: p.slice(1, n + 1).map(([x, y]) => [x, y - mean]) };
}
