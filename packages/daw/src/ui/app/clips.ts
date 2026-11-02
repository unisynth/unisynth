// Arrangement clip editing (change daw-solid-ts 9.1, step 5b): selection, clipboard, the toolbar/key commands (each
// one undo step), pointer gestures on the lanes (title bar moves, edge resizes, Alt-edge loops, body or empty space
// selects a range, double-click creates or opens), and the lane/clip menus. Clips render reactively (Clip.tsx);
// this edits the model and bumps the lanes. `initClips` binds the lanes and takes what the rest of the UI refreshes.
import type { UIClip, UITrack } from "../../model.ts";
import { raw } from "../state/song.ts";
import * as tr from "../state/transport.ts";
import * as arrange from "../state/arrange.ts";
import { laneEl, rollClips } from "../state/view.ts";
import { popMenu } from "../common/Menu.tsx";
import { carry, links, patOf } from "../model/track.ts";
import { selected, setSelected, tracks } from "./session.ts";
import { commit, lastSaid, redo, say, snap, undo } from "./core.ts";
import { arrSnap, BARS, snapTo } from "./transport.ts";
import { BW, setFocusArea } from "./arrange.ts";

type ClipEl = HTMLElement & { _clip: UIClip };
type Orig = { t: UITrack; c: UIClip; start: number; len: number; cur?: UITrack };
/** A clip gesture: move/resize the selected clips, or a marquee (range select; `pend`: a press on a clip body that
 * selects that clip unless it turns into a drag, from `x`/`yc`). */
type ADrag =
  | { mode: "move" | "resize"; bar0: number; moved?: boolean; before: string; alt: boolean; bars0: number; t0: UITrack; orig: Orig[]; dt?: number }
  | { mode: "marquee"; bar0: number; moved?: boolean; y0: number; keep: Set<UIClip>; box: HTMLElement; x?: number; yc?: number; pend?: { c: UIClip; t: UITrack; shift: boolean } };
export interface ClipHooks {
  /** Grow the song to the last clip (`quiet`: no message). */
  fitSong(quiet?: boolean): void;
  renderDevice(): void;
  selectTrack(t: UITrack): void;
  /** A clip was clicked: it becomes the track's roll clip (the roll centres on it while shown). */
  clipPicked(t: UITrack, c: UIClip): void;
  /** Show the Piano roll tab, centred on the track's roll clip. */
  showRoll(): void;
  /** The selected track's roll content changed (`redraw`: repaint it now when shown). */
  rollChanged(redraw: boolean): void;
}
let hooks: ClipHooks;
let lanesEl: HTMLElement, lw: HTMLElement, cursorEl: HTMLElement | null = null;
/** Clipboard: notes (piano roll) and clips (arrangement). */
export const clipboard: { notes: any[] | null; clips: any[] | null } = { notes: null, clips: null };
let aCursor = 13, aDrag: ADrag | null = null;
const renderLane = (t: UITrack) => arrange.bumpLane(t.id);

export function initClips(h: ClipHooks): void {
  hooks = h;
  lanesEl = document.getElementById("lanes")!; lw = document.getElementById("lanes-wrap")!;
  cursorEl = Object.assign(document.createElement("div"), { className: "cursor" }); lanesEl.insertBefore(cursorEl, lanesEl.querySelector(".after-end")); drawCursor();  // under the song-end shade, as before
  document.querySelectorAll<HTMLElement>("#arr-corner [data-arr]").forEach(b => b.onclick = () => { setFocusArea("arr"); arrCmd(b.dataset.arr!); lw.focus(); });
  lanesEl.addEventListener("pointerdown", onDown);
  lanesEl.addEventListener("pointermove", onMove);
  lanesEl.addEventListener("pointerup", onUp);
  lw.addEventListener("keydown", e => { if (e.target === lw) arrKeys(e); });
  lanesEl.addEventListener("mousedown", e => { if (e.button === 0 && (e.target as Element).closest(".lane.track")) e.preventDefault(); });
  lanesEl.addEventListener("pointerup", onUpCapture, true);
  lanesEl.addEventListener("contextmenu", onMenu);
}
export const instrTracks = (): UITrack[] => tracks.filter(t => !t.bus);
export const selClips = (): { t: UITrack; c: UIClip }[] => tracks.flatMap(t => t.clips.filter(c => c.sel).map(c => ({ t, c })));
export const clearClipSel = (): void => tracks.forEach(t => t.clips.forEach(c => c.sel = false));
// Placing a clip over others trims or removes what it covers (Ableton-style), never overlaps.
export function place(t: UITrack, nc: UIClip): void {
  const out: UIClip[] = [];
  for (const c of t.clips) {
    if (c === nc) continue;
    const a = c.start, b = c.start + c.len, x = nc.start, y = nc.start + nc.len;
    if (b <= x || a >= y) out.push(c); else if (a < x && b > y) out.push({ ...c, len: x - a }, { ...c, start: y, len: b - y, sel: false });
    else if (a < x) out.push({ ...c, len: x - a }); else if (b > y) out.push({ ...c, start: y, len: b - y });
  }
  t.clips = [...out, nc].sort((p1, p2) => p1.start - p2.start);
}
let uniq = 2;
const arr: Record<string, (arg?: any, at?: number) => void> = {
  all() { tracks.forEach(t => t.clips.forEach(c => c.sel = true)); },
  copy() { const cs = selClips(); if (!cs.length) return say("no clips selected"); const i0 = Math.min(...cs.map(x => instrTracks().indexOf(x.t))), s0 = Math.min(...cs.map(x => x.c.start));
    clipboard.clips = cs.map(({ t, c }) => ({ dt: instrTracks().indexOf(t) - i0, ds: c.start - s0, len: c.len, name: c.name, pat: structuredClone(raw(patOf(t, c.name))) })); say(`${cs.length} clips copied`); },
  del() { const n = selClips().length; tracks.forEach(t => t.clips = t.clips.filter(c => !c.sel)); say(`${n} clips deleted`); },
  cut() { arr.copy!(); arr.del!(); say(`${clipboard.clips?.length || 0} clips cut`); },
  paste(linked?: boolean, at = aCursor) { if (!clipboard.clips) return say("clipboard empty"); clearClipSel(); const it = instrTracks(), i0 = Math.max(0, it.indexOf(selected));
    for (const c of clipboard.clips) { const t = it[i0 + c.dt]; if (!t) continue;
      const name = linked ? c.name : `${c.name} ${uniq++}`; if (!t.pats?.[name]) (t.pats ??= {})[name] = structuredClone(c.pat);
      place(t, { start: at + c.ds, len: c.len, name, sel: true }); }
    say(`${clipboard.clips.length} clips pasted${linked ? " (linked)" : ""} at bar ${+at.toFixed(2)}`); },
  dup(linked?: boolean) { const cs = selClips(); if (!cs.length) return say("no clips selected"); const end = Math.max(...cs.map(x => x.c.start + x.c.len)), keep = clipboard.clips, t0 = selected;
    setSelected(instrTracks()[Math.min(...cs.map(x => instrTracks().indexOf(x.t)))]!); arr.copy!(); arr.paste!(linked, end); clipboard.clips = keep; setSelected(t0);
    say(`${cs.length} clips duplicated${linked ? " (linked)" : ""} to bar ${+end.toFixed(2)}`); },
  move(d: number) { const cs = selClips(); if (!cs.length) return; if (cs.some(x => x.c.start + d < 1)) return say("song start");
    cs.forEach(x => x.c.start += d); cs.forEach(x => place(x.t, x.c)); say(`moved ${d > 0 ? "+" : ""}${d} bar`); },
  track(d: number) { const cs = selClips(), it = instrTracks(); if (!cs.length) return; if (cs.some(x => !it[it.indexOf(x.t) + d])) return say("no track there");
    for (const x of cs) { const nt = it[it.indexOf(x.t) + d]!; carry(x.t, nt, x.c.name); x.t.clips = x.t.clips.filter(c => c !== x.c); place(nt, x.c); } say(`moved to ${d > 0 ? "next" : "previous"} track`); },
  align() { const q = arrSnap() || 1, cs = selClips().length ? selClips() : tracks.flatMap(t => t.clips.map(c => ({ t, c })));
    for (const x of cs) x.c.start = Math.max(1, 1 + Math.round((x.c.start - 1) / q) * q); cs.forEach(x => place(x.t, x.c)); say(`aligned ${cs.length} clips to ${q === 1 ? "bars" : tr.snap()}`); },
};
/** One arrangement command as one undo step (the toolbar, keys and menus). */
export function arrCmd(name: string, arg?: unknown): void { const before = snap(); arr[name]!(arg); hooks.fitSong(); commit(before, lastSaid()); tracks.forEach(renderLane); drawCursor(); hooks.renderDevice(); }
/** The arrangement cursor (paste position). */
export function drawCursor(): void { if (cursorEl) cursorEl.style.left = (aCursor - 1) * BW + "px"; }
const lpos = (e: MouseEvent): [number, number] => { const r = lanesEl.getBoundingClientRect(); return [1 + (e.clientX - r.left) / BW, e.clientY - r.top]; };
const laneAtY = (y: number): HTMLElement | undefined => Array.from(lanesEl.querySelectorAll<HTMLElement>(".lane.track")).find(l => { const r = l.getBoundingClientRect(); return y >= r.top && y < r.bottom; });
function onDown(e: PointerEvent): void {
  const tg = e.target as HTMLElement;
  if (e.button !== 0 || !tg.closest(".lane.track")) return;  // automation lanes have their own editor; right button = menu
  setFocusArea("arr"); lw.focus({ preventScroll: true }); lanesEl.setPointerCapture(e.pointerId);
  const el = tg.closest<ClipEl>(".clip"), [bar, y] = lpos(e), t = tracks.find(x => x.id === tg.closest<HTMLElement>(".lane.track")!.dataset.track)!;
  if (el) {
    const c = el._clip;
    if (!tg.closest(".lbl, .rz")) return bodyDown(e, t, c, bar, y);  // clip body: click selects, drag selects a range
    if (e.shiftKey) { c.sel = !c.sel; renderLane(t); return; }
    if (!c.sel) { clearClipSel(); c.sel = true; }
    aDrag = { mode: tg.classList.contains("rz") ? "resize" : "move", alt: e.altKey, bars0: BARS, bar0: bar, before: snap(), t0: t, orig: selClips().map(x => ({ ...x, start: x.c.start, len: x.c.len })) };
    hooks.clipPicked(t, c);
  } else {
    if (!e.shiftKey) clearClipSel();
    aCursor = Math.max(1, 1 + Math.floor((bar - 1) / (arrSnap() || 1)) * (arrSnap() || 1)); drawCursor();
    aDrag = { mode: "marquee", bar0: bar, y0: y, keep: new Set(selClips().map(x => x.c)), box: Object.assign(document.createElement("div"), { className: "marquee" }) };
    if (selected !== t) hooks.selectTrack(t);
  }
  tracks.forEach(renderLane);
}
// Press on a clip body (Ableton: the title bar moves, the body selects). Click = select the clip; drag = range.
function bodyDown(e: PointerEvent, t: UITrack, c: UIClip, bar: number, y: number): void {
  aCursor = Math.max(1, 1 + Math.floor((bar - 1) / (arrSnap() || 1)) * (arrSnap() || 1)); drawCursor();
  aDrag = { mode: "marquee", bar0: bar, y0: y, x: e.clientX, yc: e.clientY, keep: new Set(e.shiftKey ? selClips().map(x => x.c) : []), pend: { c, t, shift: e.shiftKey },
    box: Object.assign(document.createElement("div"), { className: "marquee" }) };
}
function onMove(e: PointerEvent): void {
  if (!aDrag) return; const [bar, y] = lpos(e), q = arrSnap();
  if (aDrag.mode === "marquee" && !aDrag.moved && aDrag.pend && Math.hypot(e.clientX - aDrag.x!, e.clientY - aDrag.yc!) < 4) return;
  if (aDrag.mode === "marquee" && !aDrag.moved && aDrag.pend && !aDrag.pend.shift) clearClipSel();
  aDrag.moved = true;
  if (aDrag.mode === "move") {
    let d = snapTo(bar - aDrag.bar0, q); d = Math.max(1 - Math.min(...aDrag.orig.map(o => o.start)), d);
    const it = instrTracks(), lane = laneAtY(e.clientY), tt = lane && tracks.find(x => x.id === lane.dataset.track);
    const dt = tt && !tt.bus ? it.indexOf(tt) - it.indexOf(aDrag.t0) : 0, ok = aDrag.orig.every(o => it[it.indexOf(o.t) + dt]);
    for (const o of aDrag.orig) { o.c.start = o.start + d; const nt = ok ? it[it.indexOf(o.t) + dt]! : o.t; if (nt !== o.cur) { carry(o.t, nt, o.c.name); (o.cur || o.t).clips = (o.cur || o.t).clips.filter(c => c !== o.c); nt.clips.push(o.c); o.cur = nt; } }
    aDrag.dt = ok ? dt : 0;
  } else if (aDrag.mode === "resize") { const d = snapTo(bar - aDrag.bar0, q); for (const o of aDrag.orig) { o.c.len = Math.max(q || .25, o.len + d); if (aDrag.alt) o.c.loop = true; } }
  else if (aDrag.mode === "marquee") { const x0 = (Math.min(aDrag.bar0, bar) - 1) * BW, x1 = (Math.max(aDrag.bar0, bar) - 1) * BW, y0 = Math.min(aDrag.y0, y), y1 = Math.max(aDrag.y0, y);
    Object.assign(aDrag.box.style, { left: x0 + "px", top: y0 + "px", width: x1 - x0 + "px", height: y1 - y0 + "px" }); if (!aDrag.box.isConnected) lanesEl.append(aDrag.box);
    const lr = lanesEl.getBoundingClientRect();
    for (const t of tracks) for (const c of t.clips) { const ly = laneEl.get(t.id)?.offsetTop ?? -1, lh = laneEl.get(t.id)?.offsetHeight ?? 0, cx0 = (c.start - 1) * BW, cx1 = cx0 + c.len * BW;
      c.sel = aDrag.keep.has(c) || (cx0 < x1 && cx1 > x0 && ly < y1 && ly + lh > y0); } void lr; }
  if (aDrag.mode !== "marquee") hooks.fitSong(true);
  tracks.forEach(renderLane);
}
function onUp(): void {
  if (!aDrag) return;
  if (aDrag.mode === "move" && aDrag.moved) { for (const o of aDrag.orig) place(o.cur || o.t, o.c); commit(aDrag.before, `${aDrag.orig.length} clips moved${aDrag.dt ? ` ${aDrag.dt > 0 ? "down" : "up"} ${Math.abs(aDrag.dt)} track` : ""}`); }
  if (aDrag.mode === "resize" && aDrag.moved) { for (const o of aDrag.orig) place(o.t, o.c); commit(aDrag.before, "clip length changed"); }
  if (aDrag.mode === "marquee") { aDrag.box.remove(); if (aDrag.moved) say(`${selClips().length} clips selected`);
    else if (aDrag.pend) { const { c, t, shift } = aDrag.pend; if (shift) c.sel = !c.sel; else { clearClipSel(); c.sel = true; }
      hooks.clipPicked(t, c); } }
  if (aDrag.mode === "move" || aDrag.mode === "resize") { hooks.fitSong(); if (aDrag.moved && BARS > aDrag.bars0) say(`${lastSaid()} · song extended to ${BARS} bars`); }
  aDrag = null; tracks.forEach(renderLane); hooks.renderDevice();
}
/** Arrangement keys (lanes focused, or a track header: `hdrTrack`). */
export function arrKeys(e: KeyboardEvent, hdrTrack?: UITrack): void {
  const mod = e.ctrlKey || e.metaKey, k = e.key.toLowerCase(); let cmd: string | null = null, arg: unknown;
  if (mod && k === "a") cmd = "all"; else if (mod && k === "c") cmd = "copy"; else if (mod && k === "x") cmd = "cut";
  else if (mod && k === "v") { cmd = "paste"; arg = e.shiftKey; } else if (mod && k === "d") { cmd = "dup"; arg = e.shiftKey; }
  else if (mod && k === "z") { e.preventDefault(); return e.shiftKey ? redo() : undo(); }
  else if (k === "delete" || k === "backspace") cmd = "del";
  else if (k === "arrowleft" || k === "arrowright") { cmd = "move"; arg = (k === "arrowright" ? 1 : -1) * (e.shiftKey ? 4 : arrSnap() || 1); }
  else if (k === "arrowup" || k === "arrowdown") { cmd = "track"; arg = k === "arrowdown" ? 1 : -1; }
  else if (k === "q") cmd = "align";
  else if (k === "escape") { clearClipSel(); tracks.forEach(renderLane); return; }
  if (hdrTrack && cmd === "all") { e.preventDefault(); clearClipSel(); hdrTrack.clips.forEach(c => c.sel = true); tracks.forEach(renderLane); say(`${hdrTrack.clips.length} clips of ${hdrTrack.name} selected`); return; }
  if (hdrTrack && (cmd === "move" || cmd === "track")) return;  // arrows on a header move between tracks
  if (!cmd) return; e.preventDefault(); arrCmd(cmd, arg);
}

// Clips: create, make unique, loop.
let clipN = 1;
export function createClip(t: UITrack, bar: number): void {
  if (t.bus) { say("bus tracks have no clips; they play what other tracks send"); return; }
  const q = arrSnap() >= 1 ? arrSnap() : 1, start = 1 + Math.floor((bar - 1) / q) * q, name = `clip ${clipN++}`;
  const before = snap(); clearClipSel(); (t.pats ??= {})[name] = { len: 1, notes: [], cc: { wheel: [], bend: [] } };
  const c: UIClip = { start, len: 1, name, sel: true }; place(t, c); hooks.fitSong(true); rollClips.set(t.id, c);
  tracks.forEach(renderLane); hooks.selectTrack(t); commit(before, `${name} created on ${t.name} at bar ${start}; double-click the roll to add notes`);
  hooks.showRoll();
}
export function makeUnique(t: UITrack, c: UIClip): void {
  if (links(t, c.name) < 2) { say(`${c.name} is not linked`); return; }
  const before = snap(), name = `${c.name} ${uniq++}`; t.pats![name] = structuredClone(raw(patOf(t, c.name))); c.name = name;
  tracks.forEach(renderLane); hooks.rollChanged(false); commit(before, `clip is now unique: ${name}`); hooks.renderDevice();
}
export function toggleClipLoop(t: UITrack, c: UIClip): void { const before = snap(); c.loop = !c.loop; tracks.forEach(renderLane); hooks.rollChanged(true); commit(before, `${c.name}: ${c.loop ? `loops its ${patOf(t, c.name).len}-bar pattern` : "plays once (crop)"}`); }
// Double-click: empty lane space creates a clip; a clip opens in the piano roll. (The lanes hold pointer capture
// during a press, so the lane is found from the pointer position, not from the event target.)
// Own double-click detection: a click redraws the clips, so the browser never sees two clicks on one element.
let aLastClick: { t: number; x: number; y: number } | null = null;
function onUpCapture(e: PointerEvent): void {
  if (e.button !== 0 || !aDrag || aDrag.moved) return;
  const now = performance.now(), dbl = aLastClick && now - aLastClick.t < 400 && Math.hypot(e.clientX - aLastClick.x, e.clientY - aLastClick.y) < 5;
  aLastClick = dbl ? null : { t: now, x: e.clientX, y: e.clientY };
  if (dbl) setTimeout(() => laneDouble(e));
}
function laneDouble(e: PointerEvent): void {
  const lane = laneAtY(e.clientY); if (!lane) return; const t = tracks.find(x => x.id === lane.dataset.track)!, bar = lpos(e)[0];
  const c = t.clips.find(c1 => bar >= c1.start && bar < c1.start + c1.len);
  if (c) { rollClips.set(t.id, c); hooks.selectTrack(t); hooks.showRoll(); return; }
  createClip(t, bar);
}
function onMenu(e: MouseEvent): void {
  const tg = e.target as HTMLElement, lane = tg.closest<HTMLElement>(".lane.track"); if (!lane) return; e.preventDefault();
  const t = tracks.find(x => x.id === lane.dataset.track)!, el = tg.closest<ClipEl>(".clip"), bar = lpos(e)[0];
  if (!el) return popMenu(e.clientX, e.clientY, [["Create clip here", "double-click", () => createClip(t, bar), t.bus], ["Paste here", "Ctrl+V", () => { aCursor = 1 + Math.floor(bar - 1); drawCursor(); if (selected !== t) hooks.selectTrack(t); arrCmd("paste"); }, !clipboard.clips]], "Lane menu");
  const c = el._clip; if (!c.sel) { clearClipSel(); c.sel = true; tracks.forEach(renderLane); }
  popMenu(e.clientX, e.clientY, [["Open in piano roll", "", () => { rollClips.set(t.id, c); hooks.selectTrack(t); hooks.showRoll(); }],
    ["Duplicate", "Ctrl+D", () => arrCmd("dup", false)], ["Duplicate linked", "Ctrl+Shift+D", () => arrCmd("dup", true)],
    ["Make unique", "", () => makeUnique(t, c), links(t, c.name) < 2], [c.loop ? "Loop off (crop)" : "Loop clip", "Alt+drag edge", () => toggleClipLoop(t, c)], "-",
    ["Cut", "Ctrl+X", () => arrCmd("cut")], ["Copy", "Ctrl+C", () => arrCmd("copy")], ["Delete", "Del", () => arrCmd("del")]], `Clip ${c.name} menu`);
}
