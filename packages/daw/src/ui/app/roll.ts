// Piano roll of the running session (change daw-solid-ts 9.1, step 6): the open clip's notes and controller envelopes,
// drawing (only the visible window of the canvases), note commands (one undo step each), grid and controller-lane
// gestures, audition keys, zoom. The panel's markup: ui/roll/Roll.tsx (mounted only while its tab shows).
import type { Point, UIClip, UINote, UITrack } from "../../model.ts";
import type { RollEls } from "../roll/Roll.tsx";
import * as tr from "../state/transport.ts";
import { bumpLane } from "../state/arrange.ts";
import { rollClips } from "../state/view.ts";
import { anchorScroll, clamp, wheelF } from "../zoom.ts";
import { paramOf, vget } from "../model/params.ts";
import { instOf, links, patOf } from "../model/track.ts";
import { selected } from "./session.ts";
import { commit, lastSaid, redo, say, snap, undo } from "./core.ts";
import { LOOP, rollSnap, snapTo, SONG_POS } from "./transport.ts";
import { focusArea, holdOn, holdOnBar, setFocusArea } from "./arrange.ts";
import { clipboard } from "./clips.ts";
import { css } from "./theme.ts";
import type { Snap } from "../state/snapshot.ts";

export interface RollHooks {
  /** An audition key pressed: play `note` on the selected track, held as `key`. */
  play(note: number, key: string): void;
  stop(key: string): void;
}
let hooks: RollHooks = { play() {}, stop() {} };
export function initRoll(h: RollHooks): void { hooks = h; }

/** The loop changed: redraw its shade while a clip shows. */
export function rollLoopChanged(): void { if (rollEls && rollClip) drawRoll(); }
/** While following: scrolls the playhead into the grid's view. */
export function followRoll(): void {
  if (grid && rollClip) { const rx = songStep(SONG_POS) * STEP, gl = grid.scrollLeft, gw = grid.clientWidth; if (rx >= 0 && rx <= rollLen * STEP && (rx < gl || rx > gl + gw - 30)) grid.scrollLeft = Math.max(0, rx - 30); }
}
/** A key event meant for the roll: on its canvases, or the roll is the focus area and the event is outside the arrangement. */
export function inRoll(e: Event): boolean {
  const t = e.target as Element | null;
  return t === gridc || t === velc || (focusArea === "roll" && !!rollEls && !t?.closest?.("#arr, #hdrs"));
}

// The roll's elements exist only while the Piano roll tab shows (ui/roll/Roll.tsx, design D5): null otherwise (typed
// non-null: only handlers bound on mount and code behind a `gridc` / `rollEls` check touch them). On each mount
// `attachRoll` takes them and runs the listener setups registered with `onRollMount` (in place, below).
const NONE = null as never;
let keys: HTMLElement = NONE, gridc: HTMLCanvasElement = NONE, velc: HTMLCanvasElement = NONE, grid: HTMLElement = NONE, vel: HTMLElement = NONE,
  ccKind: HTMLSelectElement = NONE, ccVal: HTMLElement = NONE, velSet: HTMLInputElement = NONE, rollEls: RollEls = NONE;
let rollScroll = [0, 0];  // the grid's scroll position across remounts (as when the panel only hid)
const rollBinds: (() => void)[] = [];
const onRollMount = (f: () => void): void => { rollBinds.push(f); };
/** Mount (ui/roll/Roll.tsx): takes the panel's elements, binds the handlers, draws, restores the scroll position. */
export function attachRoll(els: RollEls): void {
  rollEls = els; ({ keys, gridc, velc, grid, vel, ccKind, ccVal, velSet } = els); gridSize = velSize = null;
  keys.style.fontSize = KH < 10 ? "0" : "";
  for (const f of rollBinds) f();
  genRoll(selected); drawRoll(); [grid.scrollLeft, grid.scrollTop] = rollScroll; centreRoll();
}
/** Unmount: keeps the scroll position, drops the elements. */
export function detachRoll(): void { if (grid) rollScroll = [grid.scrollLeft, grid.scrollTop]; rollEls = keys = gridc = velc = grid = vel = ccKind = ccVal = velSet = NONE; gridSize = velSize = null; }
/** Pitch-class names (C, C♯ …). */
export const NAMES = ["C", "C♯", "D", "D♯", "E", "F", "F♯", "G", "G♯", "A", "A♯", "B"];
const LO = 24, HI = 96; let KH = 12, STEP = 22;  // zoomable: KH = key row height, STEP = px per 1/16
let rollNotes: UINote[] = [], rollClip: UIClip | null = null, rollLen = 64;
let rollCC: { wheel: Point[]; bend: Point[] } = { wheel: [], bend: [] };
function rollClipOf(t: UITrack): UIClip | null {
  const r = rollClips.get(t.id), keep = r && (t.clips.includes(r as UIClip) ? r as UIClip : t.clips.find(c => c.name === r.name && c.start === r.start));
  const c = keep || t.clips.find(c => c.sel) || t.clips[0] || null; rollClips.set(t.id, c); return c;
}
/** Opens `t`'s clip in the roll (the one opened before, else the selected or first clip; none on a bus). */
export function genRoll(t: UITrack): void {
  rollClip = t.bus || t.master ? null : rollClipOf(t);
  if (!rollClip) { rollNotes = []; rollCC = { wheel: [], bend: [] }; return; }
  const p = patOf(t, rollClip.name); rollNotes = p.notes; rollCC = p.cc!; rollLen = Math.max(1, Math.round((rollClip.loop ? p.len : rollClip.len) * 16));
  const hdr = rollEls?.rollClip; if (hdr) hdr.textContent = `${rollClip.name} · bar ${+rollClip.start.toFixed(2)} · ${+rollClip.len.toFixed(2)} bars${rollClip.loop ? ` · ⟳ ${p.len}-bar loop ×${+(rollClip.len / p.len).toFixed(2)}` : ""}${links(t, rollClip.name) > 1 ? ` · ⧉ ${links(t, rollClip.name)} linked` : ""}`;
}
const songStep = (bar: number): number => (bar - rollClip!.start) * 16;  // song bar -> step inside the open clip
// The roll and controller canvases cover only the visible part of the clip (a whole-song clip at full zoom would be
// tens of thousands of pixels wide: past the browser's canvas limit and hundreds of MB). `rollX0` = the clip x (px)
// at the canvas' left edge; drawing uses clip coordinates (translated), pointer positions add `rollX0`. Sizers keep
// the scroll width; scrolling redraws.
let rollX0 = 0, gridSize: HTMLElement | null = null, velSize: HTMLElement | null = null;
function rollView(W: number): number {
  if (!gridSize) { gridSize = document.createElement("div"); velSize = document.createElement("div"); gridSize.className = velSize.className = "roll-size"; grid.prepend(gridSize); vel.prepend(velSize); }
  const vw = Math.max(1, Math.min(W, Math.ceil(grid.clientWidth || innerWidth)));  // hidden panel: a screen wide, never the clip
  rollX0 = Math.max(0, Math.min(Math.floor(grid.scrollLeft), W - vw));
  return vw;
}
let rollRedraw = 0;
const redrawRollSoon = () => { if (!rollRedraw) rollRedraw = requestAnimationFrame(() => { rollRedraw = 0; drawRoll(); }); };
/** Paints the visible window of the note grid, the info line and the controller lane. */
export function drawRoll(): void {
  if (!gridc) return;  // not mounted
  if (!rollClip) { gridc.width = gridc.height = 0; velc.width = velc.height = 0; if (gridSize) gridSize.style.width = velSize!.style.width = "0"; return; }
  const W = rollLen * STEP, H = (HI - LO + 1) * KH, dpr = devicePixelRatio || 1;
  const vw = rollView(W); gridSize!.style.width = W + "px"; gridSize!.style.height = H + "px";
  gridc.width = vw * dpr; gridc.height = H * dpr; gridc.style.width = vw + "px"; gridc.style.height = H + "px"; gridc.style.transform = `translateX(${rollX0}px)`;
  const g = gridc.getContext("2d")!; g.scale(dpr, dpr); g.translate(-rollX0, 0);
  for (let n = HI; n >= LO; n--) { const y = (HI - n) * KH; g.fillStyle = NAMES[n % 12].includes("♯") ? css("--roll-black") : css("--panel"); g.fillRect(0, y, W, KH); }
  for (let s = 0; s <= rollLen; s++) { g.fillStyle = s % 16 === 0 ? css("--dim") : s % 4 === 0 ? css("--roll-beat") : css("--roll-step"); g.fillRect(s * STEP, 0, 1, H); }
  const l0 = Math.max(0, songStep(LOOP[0])), l1 = Math.min(rollLen, songStep(LOOP[1])); if (l1 > l0) { g.fillStyle = css("--loop"); g.fillRect(l0 * STEP, 0, (l1 - l0) * STEP, H); }
  for (const n of rollNotes) {
    const x = n.s * STEP + 1, y = (HI - n.n) * KH + 1;
    g.fillStyle = css(selected.col); g.globalAlpha = .45 + .55 * n.v; g.fillRect(x, y, n.l * STEP - 2, KH - 2); g.globalAlpha = 1;
    if (n.sel || n === velHover) { g.strokeStyle = n === velHover ? css("--focus") : css("--fg"); g.lineWidth = n === velHover ? 2 : 1.5; g.strokeRect(x + .5, y + .5, n.l * STEP - 3, KH - 3); }
  }
  rollEls.rollPh.style.height = H + "px"; moveRollPh();
  g.strokeStyle = css("--fg"); g.setLineDash([3, 3]); g.beginPath(); g.moveTo(rCursor * STEP + .5, 0); g.lineTo(rCursor * STEP + .5, H); g.stroke(); g.setLineDash([]);
  if (rDrag?.mode === "marquee" && rDrag.moved) { const [a1, b1] = [Math.min(rDrag.st0, rDrag.st1), Math.max(rDrag.st0, rDrag.st1)], [k1, k2] = [Math.min(rDrag.key0, rDrag.key1), Math.max(rDrag.key0, rDrag.key1)];
    g.fillStyle = "#6aa7ff22"; g.strokeStyle = css("--focus"); g.fillRect(a1 * STEP, (HI - k2) * KH, (b1 - a1) * STEP, (k2 - k1 + 1) * KH); g.strokeRect(a1 * STEP + .5, (HI - k2) * KH + .5, (b1 - a1) * STEP, (k2 - k1 + 1) * KH); }
  const ns = rollNotes.filter(n => n.sel), vs = ns.map(n => Math.max(1, Math.round(n.v * 127))), vmin = Math.min(...vs), vmax = Math.max(...vs);
  rollEls.rollInfo.textContent = ns.length ? `${ns.length} selected · ${NAMES[ns[0].n % 12]}${Math.floor(ns[0].n / 12) - 1} · vel ${vmin === vmax ? vmin : vmin + "–" + vmax}` : `${rollNotes.filter(n => n.s < rollLen).length} notes`;
  const vf = velSet; if (document.activeElement !== vf) { vf.disabled = !ns.length; vf.value = ns.length && vmin === vmax ? String(vmin) : ""; vf.placeholder = ns.length ? "mixed" : ""; }
  drawCC();
}
// Controller lane under the notes: velocity (per note, 1–127), mod wheel (0–127) and pitch bend (±range) as clip
// envelopes. Velocity: drag over stems paints, mouse wheel nudges (Shift ×8), selected notes move together.
// Wheel/bend: click adds, drag moves, double-click deletes (like automation lanes).
const VH = 84;
type CCDrag =
  | { vel: "group"; before: Snap; v0: number; orig: [UINote, number][] }
  | { vel: "one"; before: Snap; n: UINote }
  | { vel: "paint"; before: Snap; px: number; v: number; n_?: number }
  | { vel?: undefined; i: number };
let ccTip: { x: number; t: string } | null = null, ccDrag: CCDrag | null = null, ccLast: { i: number; t: number } | null = null;
// the selected track's instrument bend range (unisynth ctl.bend_range, 0..24 semitones, default 2)
const bendRange = (): number => { const d = instOf(selected); return d ? +(vget(d, paramOf(d, "ctl.bend_range")!) * 24).toFixed(2) : 2; };
const ccText = (k: string, v: number): string => k === "vel" ? `vel ${Math.max(1, Math.round(v * 127))}` : k === "wheel" ? `MW ${Math.round(v * 127)}` : `PB ${((v - .5) * 2 * bendRange()).toFixed(2)} st`;
function drawCC(): void {
  if (!velc) return;
  const W = rollLen * STEP, dpr = devicePixelRatio || 1, k = ccKind.value as "vel" | "wheel" | "bend", y = (v: number): number => VH - 4 - v * (VH - 10);
  const vw = rollView(W); velSize!.style.width = W + "px"; velSize!.style.height = VH + "px";
  velc.width = vw * dpr; velc.height = VH * dpr; velc.style.width = vw + "px"; velc.style.height = VH + "px"; velc.style.transform = `translateX(${rollX0}px)`;
  const v = velc.getContext("2d")!; v.scale(dpr, dpr); v.translate(-rollX0, 0);
  for (let s1 = 0; s1 <= rollLen; s1 += 4) { v.fillStyle = s1 % 16 === 0 ? css("--roll-beat") : css("--roll-step"); v.fillRect(s1 * STEP, 0, 1, VH); }
  if (k === "vel") {
    const sx = stems();
    for (const n of [...rollNotes.filter(x => !x.sel), ...rollNotes.filter(x => x.sel)]) { if (!sx.has(n)) continue; const x = sx.get(n)!;
      v.fillStyle = n === velHover ? css("--focus") : n.sel ? css("--fg") : css(selected.col); v.fillRect(x, y(n.v), 3, n.v * (VH - 10)); v.fillRect(x, y(n.v), 5, 2); }
  } else {
    const pts = rollCC[k], c = css(selected.col);
    if (k === "bend") { v.strokeStyle = css("--dim"); v.setLineDash([3, 3]); v.beginPath(); v.moveTo(0, y(.5)); v.lineTo(W, y(.5)); v.stroke(); v.setLineDash([]); }
    v.strokeStyle = c; v.lineWidth = 1.5; v.beginPath();
    const all: Point[] = [[0, pts[0]?.[1] ?? (k === "bend" ? .5 : 0)], ...pts, [rollLen, pts.at(-1)?.[1] ?? (k === "bend" ? .5 : 0)]];
    all.forEach(([s1, val], i) => i ? v.lineTo(s1 * STEP, y(val)) : v.moveTo(s1 * STEP, y(val))); v.stroke();
    for (const [s1, val] of pts) { v.beginPath(); v.arc(s1 * STEP, y(val), 3.5, 0, 7); v.fillStyle = css("--bg"); v.fill(); v.stroke(); }
  }
  if (ccTip) { v.font = "10px ui-monospace, monospace"; const tw = v.measureText(ccTip.t).width + 8; v.fillStyle = css("--panel"); v.fillRect(Math.min(W - tw, ccTip.x + 8), 2, tw, 14); v.fillStyle = css("--fg"); v.fillText(ccTip.t, Math.min(W - tw, ccTip.x + 8) + 4, 12); }
}
const ccPos = (e: MouseEvent): [number, number] => { const r = velc.getBoundingClientRect(); return [(e.clientX - r.left + rollX0) / STEP, Math.min(1, Math.max(0, (r.top + VH - 4 - e.clientY) / (VH - 10)))]; };
// All notes whose stem is under the pointer (chords share a stem position).
// One stem per note. Notes that start together (chords) get stems 5 px apart, lowest pitch first, so each can be set.
let velHover: UINote | null = null;
function stems(): Map<UINote, number> {
  const m = new Map<UINote, number>(), groups = new Map<number, UINote[]>();
  for (const n of rollNotes) if (n.s < rollLen) { const k = Math.round(n.s * 8); (groups.get(k) || groups.set(k, []).get(k)!).push(n); }
  for (const g of groups.values()) g.sort((a, b) => a.n - b.n).forEach((n, i) => m.set(n, n.s * STEP + 2 + i * 5));
  return m;
}
// The single note whose stem is nearest the pointer (within 4 px).
function stemAt(px: number): UINote | null { let best: UINote | null = null, d = 4.5; for (const [n, x] of stems()) { const dd = Math.abs(px - (x + 1.5)); if (dd < d) { d = dd; best = n; } } return best; }
// Paint between two pointer positions so a fast sweep does not skip stems.
function paintSpan(px0: number, v0: number, px1: number, v1: number): UINote[] {
  const [a, b] = px0 < px1 ? [px0, px1] : [px1, px0], hit: UINote[] = [];
  for (const [n, x] of stems()) { const c = x + 1.5; if (c >= a - 2 && c <= b + 2) { const f = b === a ? 1 : (c - px0) / (px1 - px0); n.v = clampV(v0 + (v1 - v0) * Math.min(1, Math.max(0, f))); hit.push(n); } }
  return hit;
}
const clampV = (v: number): number => Math.min(1, Math.max(1 / 127, v));
const velTip = (e: MouseEvent, ns: UINote[]): void => { ccTip = { x: e.offsetX + rollX0, t: ccText("vel", ns[0].v) + (ns.length > 1 ? ` · ${ns.length} notes` : "") }; ccVal.textContent = ccTip.t; };
onRollMount(() => {
velc.addEventListener("pointerdown", e => {
  velc.setPointerCapture(e.pointerId); const [st, val] = ccPos(e), k = ccKind.value as "vel" | "wheel" | "bend";
  if (k === "vel") {
    const n = stemAt(st * STEP), sel = selNotes(), before = snap(), solo = e.ctrlKey || e.metaKey;
    if (n && n.sel && sel.length > 1 && !solo) {      // group: every selected note by the same amount (Alt: all equal)
      ccDrag = { vel: "group", before, v0: val, orig: sel.map(x => [x, x.v]) };
      if (e.altKey) sel.forEach(x => x.v = clampV(val));
      velTip(e, sel);
    } else if (n) {                                   // one note: this stem only (Ctrl/⌘ inside a selection)
      ccDrag = { vel: "one", before, n }; n.v = clampV(val); velTip(e, [n]);
    } else {                                          // empty spot: sweep paints the stems it crosses
      ccDrag = { vel: "paint", before, px: st * STEP, v: val }; ccTip = null;
    }
    velHover = ccDrag.vel === "one" ? ccDrag.n : n; if (n) lastVel = n.v; drawRoll(); return;
  }
  const pts = rollCC[k], i = pts.findIndex(([s1, v1]) => Math.abs(s1 - st) < .4 && Math.abs(v1 - val) < .15), now = performance.now();
  if (i >= 0 && ccLast && ccLast.i === i && now - ccLast.t < 400) { pts.splice(i, 1); ccLast = null; drawCC(); ccVal.textContent = "point deleted"; return; }
  if (i >= 0) { ccDrag = { i }; ccLast = { i, t: now }; }
  else { const sn = Math.round(st * 4) / 4; pts.push([sn, val]); pts.sort((a1, b1) => a1[0] - b1[0]); ccDrag = { i: pts.findIndex(p1 => p1[0] === sn && p1[1] === val) }; }
  ccTip = { x: e.offsetX + rollX0, t: ccText(k, val) }; drawCC();
});
velc.addEventListener("pointermove", e => {
  if (!ccDrag) return; const [st, val] = ccPos(e), k = ccKind.value as "wheel" | "bend";
  if (ccDrag.vel === "group") { const d = val - ccDrag.v0; for (const [n, v0] of ccDrag.orig) n.v = clampV(e.altKey ? val : v0 + d); velTip(e, ccDrag.orig.map(o => o[0])); drawRoll(); return; }
  if (ccDrag.vel === "one") { ccDrag.n.v = clampV(val); velTip(e, [ccDrag.n]); lastVel = ccDrag.n.v; drawRoll(); return; }
  if (ccDrag.vel === "paint") { const hit = paintSpan(ccDrag.px, ccDrag.v, st * STEP, val); ccDrag.px = st * STEP; ccDrag.v = val; ccDrag.n_ = (ccDrag.n_ || 0) + hit.length;
    if (hit.length) { velTip(e, [hit.at(-1)!]); velHover = hit.at(-1)!; lastVel = clampV(val); } drawRoll(); return; }
  const pts = rollCC[k], i = ccDrag.i, lo = i ? pts[i - 1][0] + .25 : 0, hi = i < pts.length - 1 ? pts[i + 1][0] - .25 : rollLen;
  pts[i] = [Math.min(hi, Math.max(lo, Math.round(st * 4) / 4)), e.altKey ? val : k === "bend" && Math.abs(val - .5) < .03 ? .5 : val];
  ccTip = { x: e.offsetX + rollX0, t: ccText(k, pts[i][1]) }; ccVal.textContent = ccTip.t; drawCC();
});
velc.addEventListener("pointermove", e => { if (ccDrag || ccKind.value !== "vel") return; const n = stemAt(ccPos(e)[0] * STEP); if (n !== velHover) { velHover = n; velc.style.cursor = n ? "ns-resize" : ""; drawRoll(); } });
velc.addEventListener("pointerleave", () => { if (!ccDrag && velHover) { velHover = null; drawRoll(); } });
velc.addEventListener("pointerup", () => { if (ccDrag?.vel) commit(ccDrag.before, ccDrag.vel === "group" ? `velocity of ${ccDrag.orig.length} notes changed` : ccDrag.vel === "one" ? `velocity ${Math.round(ccDrag.n.v * 127)} on ${NAMES[ccDrag.n.n % 12]}${Math.floor(ccDrag.n.n / 12) - 1}` : "velocity painted"); ccDrag = null; ccTip = null; drawRoll(); });
velc.addEventListener("wheel", e => {
  if (e.ctrlKey || e.metaKey) return; if (ccKind.value !== "vel") return; const h = stemAt(ccPos(e)[0] * STEP); if (!h) return; const hits = [h];
  e.preventDefault(); const step = (e.shiftKey ? 8 : 1) / 127 * (e.deltaY < 0 ? 1 : -1), before = snap();
  const ns = h.sel && !(e.ctrlKey || e.metaKey) ? selNotes() : hits; ns.forEach(n => n.v = clampV(n.v + step));
  commit(before, `velocity ${step > 0 ? "+" : "−"}${e.shiftKey ? 8 : 1} on ${ns.length} note${ns.length > 1 ? "s" : ""}`); ccVal.textContent = ccText("vel", ns[0].v); drawRoll();
}, { passive: false });
ccKind.addEventListener("change", () => { velc.setAttribute("aria-label", `Controller lane: ${ccKind.selectedOptions[0].text}`); ccVal.textContent = ""; drawCC(); });
});
// The keys column and the controller lane follow the grid's scroll; a scroll past the drawn window redraws.
onRollMount(() => { grid.addEventListener("scroll", () => { keys.scrollTop = grid.scrollTop; vel.scrollLeft = grid.scrollLeft; if (Math.floor(grid.scrollLeft) !== rollX0 && rollClip) redrawRollSoon(); });
new ResizeObserver(redrawRollSoon).observe(grid);
vel.addEventListener("wheel", e => { if (!e.ctrlKey && !e.metaKey && !e.altKey && Math.abs(e.deltaX) > Math.abs(e.deltaY)) { e.preventDefault(); grid.scrollLeft += e.deltaX; } }, { passive: false }); });

type RDrag =
  | { mode: "move" | "resize"; moved?: boolean; st0: number; key0: number; before: Snap; orig: [UINote, number, number, number][]; hit: UINote; l0: number; alt?: boolean; f?: number }
  | { mode: "marquee"; moved?: boolean; st0: number; key0: number; st1: number; key1: number; keep: Set<UINote>; before?: undefined };
/** Roll cursor (paste target, step), gesture, last click (double-click), velocity of new notes. */
let rCursor = 0, rDrag: RDrag | null = null, rLast: { t: number; x: number; y: number } | null = null, lastVel = .78;
// ---- note editing ----
const selNotes = (): UINote[] => rollNotes.filter(n => n.sel);
const rpos = (e: MouseEvent): [number, number] => { const r = gridc.getBoundingClientRect(); return [(e.clientX - r.left + rollX0) / STEP, HI - Math.floor((e.clientY - r.top) / KH)]; };
const noteHit = (st: number, key: number): UINote | undefined => [...rollNotes].reverse().find(n => n.s < rollLen && n.n === key && st >= n.s && st < n.s + n.l);
const roll = {
  all() { rollNotes.forEach(n => n.sel = n.s < rollLen); },
  copy() { const ns = selNotes(); if (!ns.length) return say("no notes selected"); const s0 = Math.min(...ns.map(n => n.s)); clipboard.notes = ns.map(n => ({ ...n, s: n.s - s0 })); say(`${ns.length} notes copied`); },
  del() { const k = selNotes().length; rollNotes.splice(0, rollNotes.length, ...rollNotes.filter(n => !n.sel)); say(`${k} notes deleted`); },
  cut() { roll.copy(); roll.del(); say(`${clipboard.notes?.length || 0} notes cut`); },
  paste(at: number = rCursor) { if (!clipboard.notes) return say("clipboard empty"); rollNotes.forEach(n => n.sel = false);
    for (const n of clipboard.notes) if (at + n.s < rollLen) rollNotes.push({ ...n, s: at + n.s, l: Math.min(n.l, rollLen - at - n.s), sel: true });
    say(`${clipboard.notes.length} notes pasted at ${barStr(at)}`); },
  dup() { const ns = selNotes(); if (!ns.length) return say("no notes selected"); const s0 = Math.min(...ns.map(n => n.s)), e0 = Math.max(...ns.map(n => n.s + n.l)), q = rollSnap() || 1;
    const at = Math.ceil(e0 / q) * q; const keep = clipboard.notes; roll.copy(); roll.paste(at); clipboard.notes = keep; say(`${ns.length} notes duplicated to ${barStr(at)}`); void s0; },
  transpose(d: number) { const ns = selNotes(); if (!ns.length) return say("no notes selected"); if (ns.some(n => n.n + d < 0 || n.n + d > 127)) return say("out of MIDI range");
    ns.forEach(n => n.n += d); say(`transposed ${d > 0 ? "+" : ""}${d}`); scrollToNotes(ns); },
  move(d: number) { const ns = selNotes(); if (!ns.length) return; if (ns.some(n => n.s + d < 0 || n.s + n.l + d > rollLen)) return say("clip edge"); ns.forEach(n => n.s += d); say(`moved ${d > 0 ? "+" : ""}${d} steps`); },
  resize(d: number) { selNotes().forEach(n => n.l = Math.max(rollSnap() || .25, Math.min(rollLen - n.s, n.l + d))); say("length changed"); },
  quant(lengths?: boolean) { const q = rollSnap() || 1, str = +(rollEls?.qstr.value ?? 1), ns = selNotes().length ? selNotes() : rollNotes;
    for (const n of ns) { n.s += (Math.round(n.s / q) * q - n.s) * str; if (lengths) n.l = Math.max(q, n.l + (Math.round(n.l / q) * q - n.l) * str); n.s = Math.max(0, n.s); }
    say(`quantized ${ns.length} notes to ${tr.snap()} at ${Math.round(str * 100)}%`); },
  legato() { const ns = (selNotes().length ? selNotes() : rollNotes).slice().sort((a, b) => a.s - b.s);
    ns.forEach((n, i) => { const nx = ns.slice(i + 1).find(m => m.s > n.s); n.l = (nx ? nx.s : rollLen) - n.s; }); say("legato"); },
};
const barStr = (st: number): string => `${1 + Math.floor(st / 16)}.${1 + Math.floor((st % 16) / 4)}.${Math.round((st % 4) * 240)}`;
function scrollToNotes(ns: UINote[]): void { if (!grid) return; const top = (HI - Math.max(...ns.map(n => n.n))) * KH, bot = (HI - Math.min(...ns.map(n => n.n)) + 1) * KH;
  if (top < grid.scrollTop || bot > grid.scrollTop + grid.clientHeight) grid.scrollTop = Math.max(0, top - 40); }
type RollCmd = keyof typeof roll;
function rollCmd(name: RollCmd, arg?: number | boolean): void { const before = snap(); (roll[name] as (a?: number | boolean) => void)(arg); commit(before, lastSaid()); drawRoll(); bumpLane(selected.id); }
onRollMount(() => rollEls.tools.querySelectorAll<HTMLElement>("[data-roll]").forEach(b => b.onclick = () => {
  const a = b.dataset.roll!; setFocusArea("roll");
  if (a.startsWith("t")) rollCmd("transpose", +a.slice(1)); else if (a === "quantlen") rollCmd("quant", true); else rollCmd(a as RollCmd);
  gridc.focus();
}));
onRollMount(() => {
gridc.addEventListener("pointerdown", e => {
  setFocusArea("roll"); gridc.focus(); gridc.setPointerCapture(e.pointerId);
  const [st, key] = rpos(e), n = noteHit(st, key), now = performance.now();
  const dbl = rLast && now - rLast.t < 400 && Math.hypot(e.clientX - rLast.x, e.clientY - rLast.y) < 5;  // two clicks, no drag
  rLast = null;
  if (n) {
    if (dbl) { const before = snap(); n.sel = true; roll.del(); commit(before, "note deleted"); rLast = null; drawRoll(); return; }
    if (e.shiftKey) { n.sel = !n.sel; drawRoll(); return; }
    if (!n.sel) { rollNotes.forEach(x => x.sel = false); n.sel = true; }
    lastVel = n.v;
    rDrag = { mode: st > n.s + n.l - .35 ? "resize" : "move", st0: st, key0: key, before: snap(), orig: selNotes().map(x => [x, x.s, x.l, x.n]), hit: n, l0: n.l };
  } else {
    if (dbl) { const before = snap(), q = rollSnap() || 1, s0 = Math.floor(st / q) * q;
      rollNotes.forEach(x => x.sel = false); rollNotes.push({ s: s0, l: q, n: key, v: lastVel, sel: true }); commit(before, `note ${NAMES[key % 12]}${Math.floor(key / 12) - 1} added at ${barStr(s0)}`); rLast = null; drawRoll(); return; }
    if (!e.shiftKey) rollNotes.forEach(x => x.sel = false);
    rCursor = Math.max(0, Math.min(rollLen - 1, Math.floor(st / (rollSnap() || 1)) * (rollSnap() || 1)));
    rDrag = { mode: "marquee", st0: st, key0: key, st1: st, key1: key, keep: new Set(selNotes()) };
  }
  drawRoll();
});
gridc.addEventListener("pointermove", e => {
  if (!rDrag) return; const [st, key] = rpos(e); if (!rDrag.moved && Math.hypot(e.movementX, e.movementY) < 1 && st === rDrag.st0) return; rDrag.moved = true; const q = rollSnap();
  if (rDrag.mode === "move") { let ds = snapTo(st - rDrag.st0, q), dk = key - rDrag.key0;
    ds = Math.max(-Math.min(...rDrag.orig.map(o => o[1])), Math.min(rollLen - Math.max(...rDrag.orig.map(o => o[1] + o[2])), ds));
    dk = Math.max(LO - Math.min(...rDrag.orig.map(o => o[3])), Math.min(HI - Math.max(...rDrag.orig.map(o => o[3])), dk));
    for (const [n, s0, , k0] of rDrag.orig) { n.s = s0 + ds; n.n = k0 + dk; } }
  else if (rDrag.mode === "resize") {
    // the grabbed note snaps; the other selected notes scale by its factor (to the tick), Alt: the same amount
    const dl = snapTo(st - rDrag.st0, q), lh = Math.max(q || .25, Math.min(rollLen - rDrag.hit.s, rDrag.l0 + dl)), f = lh / rDrag.l0;
    rDrag.alt = e.altKey; rDrag.f = f;
    for (const [n, s0, l0] of rDrag.orig) n.l = n === rDrag.hit ? lh : Math.max(.25, Math.min(rollLen - s0, e.altKey ? l0 + dl : Math.round(l0 * f * 240) / 240));
    say(rDrag.orig.length > 1 && !e.altKey ? `length \u00d7${f.toFixed(2)} (${f >= 1 ? "+" : "\u2212"}${Math.abs(Math.round((f - 1) * 100))}%)` : `length ${+lh.toFixed(2)} steps`); }
  else if (rDrag.mode === "marquee") { rDrag.st1 = st; rDrag.key1 = key; const [a1, b1] = [Math.min(rDrag.st0, st), Math.max(rDrag.st0, st)], [k1, k2] = [Math.min(rDrag.key0, key), Math.max(rDrag.key0, key)];
    for (const n of rollNotes) n.sel = rDrag.keep.has(n) || (n.n >= k1 && n.n <= k2 && n.s < b1 && n.s + n.l > a1); }
  drawRoll();
});
gridc.addEventListener("pointerup", e => {
  if (rDrag && !rDrag.moved) rLast = { t: performance.now(), x: e.clientX, y: e.clientY };
  if (rDrag?.before && rDrag.moved) commit(rDrag.before, rDrag.mode === "move" ? `${rDrag.orig.length} notes moved` :
    rDrag.orig.length > 1 && !rDrag.alt ? `${rDrag.orig.length} notes: length ${rDrag.f! >= 1 ? "+" : "\u2212"}${Math.abs(Math.round((rDrag.f! - 1) * 100))}%` : "length changed");
  if (rDrag?.mode === "marquee") say(`${selNotes().length} notes selected`);
  rDrag = null; drawRoll(); bumpLane(selected.id);
});
gridc.addEventListener("keydown", e => {
  const mod = e.ctrlKey || e.metaKey, k = e.key.toLowerCase(); let cmd: RollCmd | null = null, arg: number | boolean | undefined;
  if (mod && k === "a") cmd = "all"; else if (mod && k === "c") cmd = "copy"; else if (mod && k === "x") cmd = "cut"; else if (mod && k === "v") cmd = "paste"; else if (mod && k === "d") cmd = "dup";
  else if (mod && k === "z") { e.preventDefault(); if (e.shiftKey) redo(); else undo(); return; }
  else if (k === "delete" || k === "backspace") cmd = "del";
  else if (k === "arrowup" || k === "arrowdown") { cmd = "transpose"; arg = (k === "arrowup" ? 1 : -1) * (e.shiftKey ? 12 : 1); }
  else if ((k === "arrowleft" || k === "arrowright") && e.altKey) { cmd = "resize"; arg = (k === "arrowright" ? 1 : -1) * (rollSnap() || 1); }
  else if (k === "arrowleft" || k === "arrowright") { cmd = "move"; arg = (k === "arrowright" ? 1 : -1) * (e.shiftKey ? 16 : rollSnap() || 1); }
  else if (k === "q") { cmd = "quant"; arg = e.shiftKey; }
  else if (k === "escape") { rollNotes.forEach(n => n.sel = false); drawRoll(); return; }
  if (!cmd) return; e.preventDefault(); rollCmd(cmd, arg);
});
});
// Roll up/down: mouse wheel scrolls pitch (native), keys column follows; start centred on the clip's notes.
/** Scrolls the grid to the middle of the clip's notes. */
export function centreRoll(): void { if (grid && rollNotes.length) { const mid = rollNotes.reduce((a, n) => a + n.n, 0) / rollNotes.length; grid.scrollTop = Math.max(0, (HI - mid) * KH - grid.clientHeight / 2); } }

onRollMount(() => velSet.addEventListener("change", () => { const v = Math.max(1, Math.min(127, Math.round(+velSet.value || 0))); if (!v || !selNotes().length) return;
  const before = snap(), ns = selNotes(); ns.forEach(n => n.v = v / 127); lastVel = v / 127; commit(before, `velocity ${v} on ${ns.length} notes`); velSet.blur(); drawRoll(); }));
onRollMount(() => { holdOn([grid, vel, keys]); holdOnBar(grid); });/** The roll's playhead at SONG_POS (wrapped into a looping clip). */
export function moveRollPh(): void {
  const el = rollEls?.rollPh; if (!el) return; if (!rollClip) { el.hidden = true; return; }
  let x = songStep(SONG_POS); if (rollClip.loop && x >= 0 && x < rollClip.len * 16) x %= rollLen; const hide = x < 0 || x > rollLen; if (el.hidden !== hide) el.hidden = hide; el.style.transform = `translateX(${x * STEP}px)`;
}
/** Lights an audition key while its note sounds. */
export function keyLight(note: number, on: boolean): void { keys?.children[HI - note]?.classList.toggle("on", on); }
// Piano-roll keys: press = audition on the selected track
onRollMount(() => {
keys.addEventListener("pointerdown", e => { const b = (e.target as Element).closest("button"); if (!b) return; e.preventDefault(); keys.setPointerCapture(e.pointerId); hooks.play(HI - Array.from(keys.children).indexOf(b), "mouse"); });
for (const ev of ["pointerup", "pointercancel"]) keys.addEventListener(ev, () => hooks.stop("mouse"));
keys.addEventListener("keydown", e => { const b = (e.target as Element).closest("button"); if (b && (e.key === "Enter" || e.key === " ") && !e.repeat) { e.preventDefault(); hooks.play(HI - Array.from(keys.children).indexOf(b), "kbkey"); } });
keys.addEventListener("keyup", () => hooks.stop("kbkey"));
});
// Zoom: px per 1/16 (STEP) and per key (KH), anchored at the pointer (default: the view centre).
export function zoomRoll(fx: number, fy: number, clientX?: number, clientY?: number, quiet?: boolean): void {
  if (!grid) return;
  const r = grid.getBoundingClientRect(), ax = (clientX ?? r.left + grid.clientWidth / 2) - r.left, ay = (clientY ?? r.top + grid.clientHeight / 2) - r.top;
  const sx = grid.scrollLeft, sy = grid.scrollTop, step0 = STEP, kh0 = KH;
  STEP = clamp(STEP * fx, 3, 120); KH = clamp(Math.round(KH * fy * 4) / 4, 6, 32);
  document.documentElement.style.setProperty("--kh", KH + "px"); keys.style.fontSize = KH < 10 ? "0" : "";
  drawRoll(); grid.scrollLeft = anchorScroll(sx, ax, step0, STEP); grid.scrollTop = anchorScroll(sy, ay, kh0, KH);
  if (!quiet) say(`piano roll zoom ${Math.round(STEP / 22 * 100)}% × ${Math.round(KH / 12 * 100)}%`);
}
/** Shift+Z in the roll: the clip's length and pitch range fill the view. */
export function fitRoll(): void {
  if (!rollClip || !grid) return; const ns = rollNotes.filter(n => n.s < rollLen);
  STEP = clamp((grid.clientWidth - 4) / rollLen, 3, 120);
  if (ns.length) { const lo = Math.min(...ns.map(n => n.n)) - 2, hi = Math.max(...ns.map(n => n.n)) + 2; KH = clamp(Math.floor(grid.clientHeight / (hi - lo + 1)), 6, 32); }
  document.documentElement.style.setProperty("--kh", KH + "px"); keys.style.fontSize = KH < 10 ? "0" : "";
  drawRoll(); grid.scrollLeft = 0; centreRoll(); say("zoomed to the clip");
}
// Gestures: Ctrl/⌘+wheel and trackpad pinch (Chrome/Firefox send ctrl+wheel, Safari sends gesture events).
const rollWheel = (e: WheelEvent): void => {
  if (e.ctrlKey || e.metaKey) { e.preventDefault(); zoomRoll(wheelF(e), 1, e.clientX, e.clientY, true); }
  else if (e.altKey) { e.preventDefault(); zoomRoll(1, wheelF(e), e.clientX, e.clientY, true); }
};
onRollMount(() => { grid.addEventListener("wheel", rollWheel, { passive: false }); vel.addEventListener("wheel", rollWheel, { passive: false });
keys.addEventListener("wheel", e => { if (e.ctrlKey || e.metaKey || e.altKey) { e.preventDefault(); zoomRoll(1, wheelF(e), undefined, e.clientY, true); } else grid.scrollTop += e.deltaY; }, { passive: false }); });
let g0 = 1;  // trackpad pinch (Safari gesture events) on the roll; the arrangement's: ui/app/arrange.ts
onRollMount(() => {
  grid.addEventListener("gesturestart", e => { e.preventDefault(); g0 = STEP; });
  grid.addEventListener("gesturechange", e => { e.preventDefault(); const ge = e as unknown as MouseEvent & { scale: number }; zoomRoll(g0 * ge.scale / STEP, 1, ge.clientX, ge.clientY, true); });
});

onRollMount(() => rollEls.tools.querySelectorAll<HTMLElement>("[data-zoom]").forEach(b => b.onclick = () => ({ "t+": () => zoomRoll(1.5, 1), "t-": () => zoomRoll(1 / 1.5, 1), "p+": () => zoomRoll(1, 1.25), "p-": () => zoomRoll(1, .8), fit: fitRoll } as Record<string, () => void>)[b.dataset.zoom!]()));
