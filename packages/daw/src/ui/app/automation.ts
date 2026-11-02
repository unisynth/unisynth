// Automation (change daw-solid-ts 9.1, step 5a; add-synth-daw D8f): lane values at the playhead, automated knobs
// (ui/state/device.ts), the ↺ re-enable button, recording knob moves into lanes, and the automation lane rows of the
// arrangement (header + SVG lane with its point editor, imperative per design D4; ui/arrange/Tracks.tsx mounts them).
import type { Device, LanePoint, UITrack } from "../../model.ts";
import type { Param } from "../../engine.ts";
import { raw } from "../state/song.ts";
import * as tr from "../state/transport.ts";
import * as devst from "../state/device.ts";
import * as arrange from "../state/arrange.ts";
import { overrides, overrideSet } from "../state/view.ts";
import { autoParams, ip, paramOf, vget } from "../model/params.ts";
import { devicesOf, laneText, type UIDevice } from "../model/track.ts";
import { selected, tracks } from "./session.ts";
import { announce, announced, commit, say, snap } from "./core.ts";
import { BARS, PAD_BARS, SONG_POS } from "./transport.ts";
import { livePush, pushSong } from "./engine.ts";
import { BW } from "./arrange.ts";
import { css } from "./theme.ts";

type Lane = UITrack["lanes"][number];
/** A mounted automation lane row: redraw, refresh its value readout. */
export interface LaneView { draw(focus?: boolean): void; val(): void; t: UITrack; ln: Lane; ah: HTMLElement }
/** Mounted automation lane rows (rows off screen are not here). */
export const laneVals: LaneView[] = [];

let hooks = { rebuildTracks() {} };
export function initAutomation(h: typeof hooks): void { hooks = h; }

/** The value a control shows now (its automation at the playhead while automated), else the stored one. Store reads:
 * screens redraw from them. */
export const curVal = (d: Device, name: string): number => devst.autoValue(d.values, name) ?? vget(d, ip(name));
/** Knob grabbed while recording: written every frame while held (`recordFrame`). */
export function grab(t: UITrack, d: Device, p: Param): void { if (recOn) recHold = { t, d, p }; }
export function release(t: UITrack, d: Device, p: Param): void { if (recHold) recLast.delete(`${t.id}:${t.devices.indexOf(d)}:${p.name}`); recHold = null; }
/** Per playing frame: a held knob keeps writing its lane. */
export function recordFrame(): void { if (recOn && recHold) recordPoint(recHold.t, recHold.d, recHold.p, curVal(recHold.d, recHold.p.name)); }
/** A lane's value at a bar (linear between points, held before the first and after the last). */
export function laneValue(pts: LanePoint[], bar: number): number {
  pts = raw(pts);  // read-only, runs per lane per frame
  if (bar <= pts[0]![0]) return pts[0]![1];
  for (let i = 1; i < pts.length; i++) if (bar <= pts[i]![0]) { const [b0, v0] = pts[i - 1]!, [b1, v1] = pts[i]!; return v0 + (v1 - v0) * (bar - b0) / (b1 - b0); }
  return pts[pts.length - 1]![1];
}
// Automated knobs show their lane's value at the playhead (ui/state/device.ts, view state, not saved). An automated
// knob turned by hand is "overridden" and stops following its lane until re-enabled (Ableton's rule).
let autoSeen = new Map<string, Target>();  // "track:dev:param" → the target written by the last pass (cleared once its lane is gone)
type Target = { store: Record<string, number>; field: string };
function paramTarget(t: UITrack, ln: Lane): Target | null { const d = t.devices[ln.dev]; return d && paramOf(d, ln.param) ? { store: d.values, field: ln.param } : null; }
export function applyAutomation(): void {
  const seen = new Map<string, Target>();
  for (const t of tracks) for (const ln of t.lanes || []) {
    const tg = paramTarget(t, ln); if (!tg) continue;
    const key = ln.dev + ":" + ln.param, ovr = !!overrides.get(t.id)?.has(key);
    // a store write only when the value or override state changed (this runs every frame while playing)
    devst.setAuto(tg.store, tg.field, ovr ? undefined : laneValue(ln.pts, SONG_POS), ovr); seen.set(t.id + ":" + key, tg);
  }
  for (const [k, tg] of autoSeen) if (!seen.has(k)) devst.setAuto(tg.store, tg.field, undefined, false);
  autoSeen = seen;
  for (const l of laneVals) l.ah.classList.toggle("ovr", !!overrides.get(l.t.id)?.has(l.ln.dev + ":" + l.ln.param));
}
/** The ↺ Re-enable automation button of a track with overridden knobs (in the device panel header). */
export function updateReauto(t: UITrack = selected): void {
  const ctx = document.getElementById("ctx"); if (!ctx) return; ctx.querySelector("#reauto")?.remove(); if (!overrides.get(t.id)?.size) return;
  const b = Object.assign(document.createElement("button"), { id: "reauto", textContent: `↺ Re-enable automation (${overrides.get(t.id)!.size})`, title: "Knobs you turned by hand stopped following their lanes; click to follow them again" });
  b.onclick = () => { overrides.get(t.id)!.clear(); applyAutomation(); updateReauto(t); announce(`${t.name}: automation re-enabled`); }; ctx.append(b);
}
// ---- Automation record (●, Shift+R; design D8f). While playing, a device knob you turn or hold writes its lane at
// the playhead on a 1/64-bar grid, creating the lane if needed. Touch overwrite: points between this gesture's last
// write and the playhead are replaced, so holding a knob still flattens the old envelope as the song passes. The
// knob keeps following its lane (no override); the engine gets the lane at most every 200 ms; the knob's gesture is
// one undo step. A seek or loop wrap starts a new pass (a write behind the last one never deletes backwards). ----
export let recOn = false;
let recHold: { t: UITrack; d: Device; p: Param } | null = null, recPushAt = 0;
const recLast = new Map<string, number>();  // gesture key → bar of its last write
export function setRec(on: boolean): void {
  recOn = on; tr.setRec(on); recLast.clear(); document.body.classList.toggle("recording", on);
  say(on ? "automation record on: knobs turned while playing write their lanes" : "automation record off");
}
/** Writes the knob value at the playhead while recording; false when the parameter cannot be automated. */
export function recordPoint(t: UITrack, d: Device, p: Param, v: number): boolean {
  const di = t.devices.indexOf(d);
  if (di < 0 || p.structural || !autoParams(d).some(x => x.name === p.name)) return false;
  const key = `${t.id}:${di}:${p.name}`, b = 1 + Math.round((SONG_POS - 1) * 64) / 64;
  let ln = t.lanes.find(x => x.dev === di && x.param === p.name), fresh = false;
  // a new lane starts with its point: the push mounts the lane at once (store), which draws it
  if (!ln) { ln = { dev: di, param: p.name, pts: [[b, v]] }; t.lanes.push(ln); t.open = true; fresh = true; }
  const last = recLast.get(key), from = last !== undefined && last <= b ? last : b;
  const keep = ln.pts.filter(([x]) => x <= from && x !== b || x > b); keep.push([b, v]); keep.sort((a, c) => a[0] - c[0]); ln.pts = keep;
  if (recHold?.d === d && recHold.p === p) recLast.set(key, b); else recLast.delete(key);  // a pass lasts while the knob is held
  overrides.get(t.id)?.delete(`${di}:${p.name}`);
  if (fresh) hooks.rebuildTracks(); else { const l = laneVals.find(x => x.ln === ln); l?.draw(); l?.val(); }
  const now = performance.now(); if (now - recPushAt > 200) { recPushAt = now; pushSong(); } else livePush();
  return true;
}
// A hand edit: overrides automation on that knob, marks the preset edited (*).
export function knobEdited(t: UITrack, d: UIDevice, p: Param): void {
  const di = t.devices.indexOf(d), key = `${di}:${p.name}`;
  if (t.lanes.some(l => l.dev === di && l.param === p.name)) { overrideSet(t.id).add(key); applyAutomation(); updateReauto(t); }
  if (d.kind === 64 && !d._edited) { d._edited = true; arrange.setLabelRev(n => n + 1); }  // the panel and the header show the * on the edited preset
}
// One automation lane: header (.thdr.auto) and lane (.lane.auto) elements for the row (ui/arrange/Tracks.tsx inserts
// them, one pair per t.lanes entry); the SVG draw and the point editor stay imperative (design D4). Returns the
// elements and a dispose that unregisters the lane from laneVals.
export function addLane(t: UITrack, ln: Lane): { ah: HTMLElement; al: HTMLElement; dispose(): void } {
  // the target list (every parameter of every device, hundreds of options) is built when the select is first used;
  // until then it holds the current target only (rows mount while scrolling: design D6)
  const names = devicesOf(t), opts = () => { const used = new Set(t.lanes.filter(x => x !== ln).map(x => x.dev + ":" + x.param));
    return t.devices.map((dv, di) => `<optgroup label="${names[di]}">${autoParams(dv).map(p =>
      `<option value="${di}:${p.name}"${di === ln.dev && p.name === ln.param ? " selected" : ""}${used.has(di + ":" + p.name) ? " disabled" : ""}>${names[di]} · ${p.name}</option>`).join("")}</optgroup>`).join(""); };
  const ah = document.createElement("div"); ah.className = "thdr auto";
  const cur = () => laneText(t, ln, laneValue(ln.pts, SONG_POS));
  ah.innerHTML = `<label class="tgt"><span aria-hidden="true">↳</span><select aria-label="${t.name} automation target"><option value="${ln.dev}:${ln.param}" selected>${names[ln.dev]} · ${ln.param}</option></select></label>
    <span class="num val" title="Value at the playhead">${cur()}</span>
    <button class="rm" aria-label="Remove ${devicesOf(t)[ln.dev]} ${ln.param} lane" title="Remove lane">✕</button>`;
  const al = document.createElement("div"); al.className = "lane auto"; al.setAttribute("role", "group"); al.setAttribute("aria-label", `${t.name} ${devicesOf(t)[ln.dev]} ${ln.param} automation`);
  // Point editor. Mouse: click empty = add, drag point = move (snap 1/16 bar, Alt = free; time stays between
  // neighbours), double-click / right-click point = delete. Keyboard (point focused): ←/→ time, ↑/↓ value
  // (Shift = fine), Delete/Backspace = delete. Every gesture is one undo step in the real app.
  const H = 40, X = (b: number) => (b - 1) * BW, Y = (v: number) => H - 4 - v * (H - 8);
  type Drag = { i: number; moved: boolean; x0: number; y0: number; before: string; add?: false } | { add: true; x0: number; y0: number; before: string };
  let selI = -1, drag: Drag | null = null, lastDown: { i: number; t: number } | null = null;
  const fmtBar = (b: number) => { const t = Math.round((b - 1) * 16); return `${1 + Math.floor(t / 16)}.${1 + Math.floor((t % 16) / 4)}.${(t % 4) * 240}`; };
  const draw = (focus = false): void => {
    const lp = raw(ln.pts), dname = devicesOf(t)[ln.dev];  // read-only: raw points, name once (not per point)
    const W = (BARS + PAD_BARS) * BW;
    const pts: LanePoint[] = [[1, lp[0]![1]], ...lp, [BARS + 1, lp[lp.length - 1]![1]]];
    const c = css(t.col);
    al.innerHTML = `<svg width="${W}" height="${H}"><polygon fill="${c}1f" points="${X(1)},${H} ${pts.map(([b, v]) => `${X(b)},${Y(v)}`).join(" ")} ${X(BARS + 1)},${H}"/><polyline fill="none" stroke="${c}" stroke-width="1.5" points="${pts.map(([b, v]) => `${X(b)},${Y(v)}`).join(" ")}"/>${
      lp.map(([b, v], i) => `<g class="pt${i === selI ? " sel" : ""}" data-i="${i}" tabindex="0" role="slider" aria-label="${dname} ${ln.param} point ${i + 1} at ${fmtBar(b)}" aria-valuemin="0" aria-valuemax="1" aria-valuenow="${v.toFixed(3)}" aria-valuetext="${laneText(t, ln, v)} at bar ${fmtBar(b)}"><circle class="hit" cx="${X(b)}" cy="${Y(v)}" r="8"/><circle class="dot" cx="${X(b)}" cy="${Y(v)}" r="${i === selI ? 4 : 3}" fill="${i === selI ? c : css("--bg")}" stroke="${c}"/></g>`).join("")
    }${drag && !drag.add && drag.moved ? (() => { const [b, v] = lp[(drag as { i: number }).i]!; const tx = Math.min(W - 120, X(b) + 8); return `<rect x="${tx - 3}" y="2" width="118" height="14" rx="3" fill="${css("--panel")}" stroke="${css("--line")}"/><text x="${tx}" y="12" font-size="10" fill="${css("--fg")}" font-family="ui-monospace,monospace">${fmtBar(b)} · ${laneText(t, ln, v)}</text>`; })() : ""}</svg>`;
    valEl.textContent = cur(); applyAutomation();
    if (focus && selI >= 0) al.querySelector<SVGElement>(`.pt[data-i="${selI}"]`)?.focus();
  };
  const valEl = ah.querySelector(".val")!;
  const entry: LaneView = { draw, t, ln, ah, val: () => { valEl.textContent = cur(); } }; laneVals.push(entry); draw();
  const toBV = (e: PointerEvent, free: boolean): [number, number] => {
    const r = al.getBoundingClientRect();
    const raw = 1 + (e.clientX - r.left) / BW, b = free ? raw : 1 + Math.round((raw - 1) * 16) / 16;
    return [Math.min(BARS + 1, Math.max(1, b)), Math.min(1, Math.max(0, (r.top + H - 4 - e.clientY) / (H - 8)))];
  };
  const clampTime = (i: number, b: number): number => {
    const lo = i > 0 ? ln.pts[i - 1]![0] + 1 / 64 : 1, hi = i < ln.pts.length - 1 ? ln.pts[i + 1]![0] - 1 / 64 : BARS + 1;
    return Math.min(hi, Math.max(lo, b));
  };
  const remove = (i: number): void => { const before = snap(); setTimeout(() => commit(before, `${ln.param}: point deleted`)); if (ln.pts.length === 1) { status2(`${ln.param}: last point kept; remove the lane with ✕ instead`); return; } ln.pts.splice(i, 1); selI = Math.min(i, ln.pts.length - 1); draw(true); status2(`point deleted`); };
  const status2 = (m: string) => { announce(`${t.name} ${ln.param}: ${m}`); };
  al.addEventListener("pointerdown", e => {
    if (e.button !== 0) return;
    const g = (e.target as Element).closest<SVGElement>(".pt");
    al.setPointerCapture(e.pointerId);
    if (g) {
      const i = +g.dataset.i!, now = performance.now();
      if (lastDown && lastDown.i === i && now - lastDown.t < 400) { lastDown = null; drag = null; remove(i); return; }  // double-click
      lastDown = { i, t: now };
      selI = i; drag = { i, moved: false, x0: e.clientX, y0: e.clientY, before: snap() }; draw();
    }
    else drag = { add: true, x0: e.clientX, y0: e.clientY, before: snap() };
  });
  al.addEventListener("pointermove", e => {
    if (!drag || drag.add) return;
    const dg = drag;
    if (!dg.moved && Math.hypot(e.clientX - dg.x0, e.clientY - dg.y0) < 3) return;
    dg.moved = true;
    const [b, v] = toBV(e, e.altKey);
    ln.pts[dg.i] = [clampTime(dg.i, b), v]; draw();
  });
  al.addEventListener("pointerup", e => {
    if (drag?.add && Math.hypot(e.clientX - drag.x0, e.clientY - drag.y0) < 3) {
      const [b, v] = toBV(e, e.altKey);
      ln.pts.push([b, v]); ln.pts.sort((p1, p2) => p1[0] - p2[0]); selI = ln.pts.findIndex(p => p[0] === b && p[1] === v); status2(`point added at ${fmtBar(b)}`);
    } else if (drag && !drag.add && drag.moved) status2(`point moved to ${fmtBar(ln.pts[drag.i]![0])}, ${laneText(t, ln, ln.pts[drag.i]![1])}`);
    if (drag?.before) commit(drag.before, announced());
    drag = null; draw(true);
  });
  al.addEventListener("contextmenu", e => { const g = (e.target as Element).closest<SVGElement>(".pt"); if (g) { e.preventDefault(); remove(+g.dataset.i!); } });
  al.addEventListener("focusin", e => { const g = (e.target as Element).closest<SVGElement>(".pt"); if (g && +g.dataset.i! !== selI) { selI = +g.dataset.i!; draw(true); } });
  al.addEventListener("keydown", e => {
    const g = (e.target as Element).closest<SVGElement>(".pt"); if (!g) return;
    const i = +g.dataset.i!; let [b, v] = ln.pts[i]!;
    const dv = e.shiftKey ? 0.005 : 0.05, db = e.shiftKey ? 1 / 64 : 1 / 16;
    if (e.key === "ArrowUp") v = Math.min(1, v + dv); else if (e.key === "ArrowDown") v = Math.max(0, v - dv);
    else if (e.key === "ArrowRight") b = clampTime(i, b + db); else if (e.key === "ArrowLeft") b = clampTime(i, b - db);
    else if (e.key === "Delete" || e.key === "Backspace") { e.preventDefault(); remove(i); return; }
    else return;
    e.preventDefault(); const before = snap(); ln.pts[i] = [b, v]; selI = i; draw(true); commit(before, `${ln.param} point ${fmtBar(b)} · ${laneText(t, ln, v)}`);
  });
  const sl = ah.querySelector<HTMLSelectElement>(".tgt select")!;
  { let full = false;
    const fill = () => { if (full) return; full = true; sl.innerHTML = opts(); };
    for (const ev of ["pointerdown", "focus", "keydown"]) sl.addEventListener(ev, fill); }
  sl.addEventListener("change", () => {
    const [di, p] = sl.value.split(":") as [string, string], before = snap(); ln.dev = +di; ln.param = p; draw(); commit(before, `${t.name}: lane now ${devicesOf(t)[ln.dev]} · ${p}`);
  });
  ah.querySelector(".rm")!.addEventListener("click", () => {
    const before = snap(); t.lanes.splice(t.lanes.indexOf(ln), 1); applyAutomation(); commit(before, `${t.name}: lane ${ln.param} removed`);
  });
  return { ah, al, dispose: () => { const i = laneVals.indexOf(entry); if (i >= 0) laneVals.splice(i, 1); } };
}
// "+ lane": the next free automatable parameter (filter cutoff first), one undo step; the row shows it (store).
export function addAutoLane(t: UITrack): void {
  const used = new Set(t.lanes.map(x => x.dev + ":" + x.param));
  const pref = ["filt1.cutoff", "filt1.resonance", "master", "dry_wet", "feedback"];
  const cand = t.devices.flatMap((dv, di) => autoParams(dv).map(p => [di, p] as const)).filter(([di, p]) => !used.has(di + ":" + p.name));
  const free = cand.find(([, p]) => pref.includes(p.name)) || cand[0];
  if (!free) { announce(`${t.name}: ${t.devices.length ? "every automatable parameter already has a lane" : "add a device first"}`); return; }
  const [di, p] = free, d = t.devices[di]!, before = snap(), ln: Lane = { dev: di, param: p.name, pts: [[1, d.values[p.name] ?? p.default]] };
  t.lanes.push(ln); commit(before, `${t.name}: lane ${devicesOf(t)[di]} · ${p.name} added`);
}
