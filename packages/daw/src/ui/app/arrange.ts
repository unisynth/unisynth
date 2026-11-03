// Arrangement view of the running session (change daw-solid-ts 9.1, step 5): horizontal zoom (`BW`), the editor with
// focus, the ruler with the loop strip (drag, arrows, field), song length, playheads and follow scrolling, header/lane
// scroll sync, lane highlights while playing, arrangement zoom gestures. Rows themselves: ui/arrange/Tracks.tsx.
import type { UIClip } from "../../model.ts";
import * as tr from "../state/transport.ts";
import * as arrange from "../state/arrange.ts";
import * as liveState from "../state/live.ts";
import { laneEl } from "../state/view.ts";
import { anchorScroll, clamp, wheelF } from "../zoom.ts";
import { tracks } from "./session.ts";
import { lastSaid, say } from "./core.ts";
import { arrSnap, BARS, fmtBars, followHold, holdFollow, LOOP, loopOn, PAD_BARS, playing, resumeFollow, setBars, setLoopOn, snapTo, SONG_POS } from "./transport.ts";
import { liveLoop, seek } from "./engine.ts";
import { applyAutomation, laneVals } from "./automation.ts";
import { drawCursor, selClips } from "./clips.ts";

/** Arrangement px per bar. */
export let BW = 44;
export function setBW(v: number): void { BW = v; }
/** The editor that keys and zoom act on: the arrangement or the piano roll. */
export let focusArea: "arr" | "roll" = "arr";
export function setFocusArea(v: "arr" | "roll"): void { focusArea = v; }

export interface ArrangeHooks {
  /** The loop changed: the piano roll redraws its loop shade while shown. */
  rollLoop(): void;
  /** While following: the roll scrolls its playhead into view. */
  rollFollow(): void;
  /** The roll's playhead for SONG_POS. */
  rollPos(): void;
}
let hooks: ArrangeHooks = { rollLoop() {}, rollFollow() {}, rollPos() {} };
/** The arrangement's elements (set by initArrange): ruler, track headers, lanes and their scroller. */
export let ruler: HTMLElement, hdrs: HTMLElement, lanes: HTMLElement, lw: HTMLElement;
let lp: HTMLElement, shade: HTMLElement, ph: HTMLElement, ph2: HTMLElement, afterEl: HTMLElement;

/** Builds the ruler's loop strip, playheads and song-end shade and binds the arrangement's scroll, wheel, loop and
 * follow handlers. Call once, before the rows mount. */
export function initArrange(h: ArrangeHooks): void {
  hooks = h;
  ruler = document.getElementById("ruler")!; hdrs = document.getElementById("hdrs")!; lanes = document.getElementById("lanes")!; lw = document.getElementById("lanes-wrap")!;
  lp = Object.assign(document.createElement("div"), { className: "loop", tabIndex: 0 }); lp.setAttribute("role", "group"); ruler.append(lp);
  shade = Object.assign(document.createElement("div"), { className: "loopshade" }); lanes.append(shade);
  ph = Object.assign(document.createElement("div"), { className: "playhead" }); lanes.append(ph);
  ph2 = ph.cloneNode() as HTMLElement; ruler.append(ph2);
  afterEl = Object.assign(document.createElement("div"), { className: "after-end" }); lanes.append(afterEl);
  ruler.prepend(Object.assign(document.createElement("div"), { className: "loopstrip" }));
  lw.addEventListener("scroll", () => { ruler.style.transform = `translateX(${-lw.scrollLeft}px)`; hdrs.scrollTop = lw.scrollTop; });
  // One scroller: the lanes. Headers only mirror lw.scrollTop (they never write back, which made momentum scrolling
  // jump back a frame). Wheel over the headers scrolls the lanes; focusing a header scrolls the lanes to it.
  hdrPad(); new ResizeObserver(hdrPad).observe(lw);
  hdrs.addEventListener("wheel", e => { if (e.ctrlKey || e.metaKey) return; e.preventDefault(); lw.scrollTop += e.deltaY; lw.scrollLeft += e.deltaX; }, { passive: false });
  hdrs.addEventListener("focusin", e => {
    const hd = (e.target as Element).closest<HTMLElement>(".thdr"); if (!hd) return; const top = hd.offsetTop, bot = top + hd.offsetHeight;
    if (top < lw.scrollTop) lw.scrollTop = top; else if (bot > lw.scrollTop + lw.clientHeight) lw.scrollTop = bot - lw.clientHeight;
    requestAnimationFrame(() => { hdrs.scrollTop = lw.scrollTop; });
  });
  // Loop: top strip of the ruler. Drag the bar to move, its edges to resize, empty strip to draw a new loop.
  ruler.addEventListener("pointerdown", rulerDown); ruler.addEventListener("pointermove", rulerMove); ruler.addEventListener("pointerup", rulerUp);
  lp.addEventListener("keydown", loopKey);
  holdOn([lw, hdrs, ruler]); holdOnBar(lw);
  // Gestures: Ctrl/⌘+wheel and trackpad pinch (Chrome/Firefox send ctrl+wheel, Safari sends gesture events).
  lw.addEventListener("wheel", e => { if (e.ctrlKey || e.metaKey) { e.preventDefault(); zoomArr(wheelF(e), e.clientX, true); } }, { passive: false });
  ruler.addEventListener("wheel", e => { e.preventDefault(); if (Math.abs(e.deltaX) > Math.abs(e.deltaY) && !e.ctrlKey) lw.scrollLeft += e.deltaX; else zoomArr(wheelF(e), e.clientX, true); }, { passive: false });
  let g0 = 1;
  lw.addEventListener("gesturestart", e => { e.preventDefault(); g0 = BW; });
  lw.addEventListener("gesturechange", e => { e.preventDefault(); zoomArr(g0 * (e as unknown as { scale: number }).scale / BW, (e as unknown as MouseEvent).clientX, true); });
}
/** Mounted rows refill (add/delete/reorder/undo): relayout, automation, header padding. `ids`: only these tracks'
 *  rows refill (a partial undo restore: track list and song length unchanged, so no relayout). */
export function rebuildTracks(ids?: readonly string[]): void {
  if (ids) { arrange.bumpTracks(ids); if (lw) applyAutomation(); return; }  // same lane width: no hdrPad
  arrange.setRev((v) => v + 1);
  if (!lw) return;  // before initArrange
  relayout(); applyAutomation(); hdrPad(); hdrs.scrollTop = lw.scrollTop;
}
// One scroller: the lanes. Headers only mirror lw.scrollTop (they never write back, which made momentum scrolling
// jump back a frame). Wheel over the headers scrolls the lanes; focusing a header scrolls the lanes to it.
const hdrPad = (): void => { hdrs.style.paddingBottom = (lw.offsetHeight - lw.clientHeight) + 40 + "px"; };  // room for lw's horizontal scrollbar
/** Grows the song to cover the last clip (never shrinks by itself). */
export function fitSong(quiet?: boolean): void {
  const end = Math.max(1, ...tracks.flatMap(t => t.clips.map(c => c.start + c.len))), need = Math.ceil(end - 1 - 1e-9);
  if (need > BARS) { setBars(need); relayout(); if (!quiet) say(`${lastSaid()} · song extended to ${BARS} bars`); }
}
/** Ruler bars, song end and lane width for the song length and zoom. */
export function relayout(): void {
  const W = (BARS + PAD_BARS) * BW;
  ruler.style.width = lanes.style.width = W + "px";
  ruler.querySelectorAll(".bar, .end").forEach(e => e.remove());
  const every = BW < 12 ? 8 : BW < 20 ? 4 : BW < 30 ? 2 : 1;
  for (let b1 = 0; b1 < BARS + PAD_BARS; b1++) { if (b1 % every) continue; const d = document.createElement("div"); d.className = "bar num"; d.style.left = b1 * BW + "px"; d.textContent = String(b1 + 1); if (b1 >= BARS) d.style.opacity = ".4"; ruler.prepend(d); }
  const end = Object.assign(document.createElement("div"), { className: "end", textContent: "end" }); end.style.left = BARS * BW + "px"; ruler.append(end);
  afterEl.style.left = BARS * BW + "px"; afterEl.style.width = PAD_BARS * BW + "px";
  tr.setBars(BARS); laneVals.forEach(l => l.draw());
}
/** Song length field: never below the last clip. */
export function onLength(v: number): void {
  const last = Math.ceil(Math.max(1, ...tracks.flatMap(t => t.clips.map(c => c.start + c.len))) - 1 - 1e-9);
  if (v < last) { setBars(last); say(`length kept at ${last} bars: clips reach bar ${last + 1}`); } else { setBars(v); say(`song length ${v} bars`); }
  relayout();
}
// Loop: top strip of the ruler. Drag the bar to move, its edges to resize, empty strip to draw a new loop.
const loopSnap = () => arrSnap() || 0;
const snapBar = (b: number): number => 1 + snapTo(b - 1, loopSnap());
/** Loop strip, shade and the transport bar for LOOP / loopOn; the engine follows. */
export function updateLoop(msg?: string): void {
  lp.style.left = shade.style.left = (LOOP[0] - 1) * BW + "px"; lp.style.width = shade.style.width = (LOOP[1] - LOOP[0]) * BW + "px";
  lp.classList.toggle("off", !loopOn); shade.classList.toggle("off", !loopOn);
  lp.setAttribute("aria-label", `Loop ${fmtBars(LOOP[0])} to ${fmtBars(LOOP[1])}${loopOn ? "" : ", off"}. Arrows move it, Shift+arrows change its end.`);
  tr.setLoop([LOOP[0], LOOP[1]]); tr.setLoopOn(loopOn);  // the bar shows them
  hooks.rollLoop();
  if (msg) say(msg);
  liveLoop();
}
type LDrag = { seek: true } | { seek?: false; mode: "a" | "b" | "move" | "new"; b0: number; a: number; b: number; anchor: number };
let lDrag: LDrag | null = null;
const rbar = (e: MouseEvent): number => 1 + (e.clientX - ruler.getBoundingClientRect().left) / BW;
function rulerDown(e: PointerEvent): void {
  ruler.setPointerCapture(e.pointerId); const b1 = rbar(e), top = e.clientY - ruler.getBoundingClientRect().top < 10;
  if (!top) { lDrag = { seek: true }; seek(b1); return; }
  const x0 = (LOOP[0] - 1) * BW, x1 = (LOOP[1] - 1) * BW, x = (b1 - 1) * BW;
  const mode = Math.abs(x - x0) < 6 ? "a" : Math.abs(x - x1) < 6 ? "b" : x > x0 && x < x1 ? "move" : "new";
  const ld = { mode, b0: b1, a: LOOP[0], b: LOOP[1], anchor: snapBar(b1) } as const; lDrag = ld;
  if (mode === "new") { LOOP[0] = LOOP[1] = ld.anchor; }
  setLoopOn(true); updateLoop();
}
function rulerMove(e: PointerEvent): void {
  const b1 = rbar(e);
  if (!lDrag) { const x = (b1 - 1) * BW, top = e.clientY - ruler.getBoundingClientRect().top < 10, x0 = (LOOP[0] - 1) * BW, x1 = (LOOP[1] - 1) * BW;
    ruler.style.cursor = !top ? "pointer" : Math.abs(x - x0) < 6 || Math.abs(x - x1) < 6 ? "ew-resize" : x > x0 && x < x1 ? "grab" : "copy"; return; }
  if (lDrag.seek) { seek(b1); return; }
  const min = loopSnap() || .25, endMax = BARS + PAD_BARS + 1;
  if (lDrag.mode === "move") { const d = Math.max(1 - lDrag.a, Math.min(endMax - lDrag.b, snapTo(b1 - lDrag.b0, loopSnap()))); LOOP[0] = lDrag.a + d; LOOP[1] = lDrag.b + d; }
  else if (lDrag.mode === "a") LOOP[0] = Math.max(1, Math.min(LOOP[1] - min, snapBar(b1)));
  else if (lDrag.mode === "b") LOOP[1] = Math.min(endMax, Math.max(LOOP[0] + min, snapBar(b1)));
  else { const c1 = Math.max(1, snapBar(b1)); [LOOP[0], LOOP[1]] = c1 >= lDrag.anchor ? [lDrag.anchor, Math.max(c1, lDrag.anchor + min)] : [c1, lDrag.anchor]; }
  updateLoop(); ruler.style.cursor = lDrag.mode === "move" ? "grabbing" : "ew-resize";
}
function rulerUp(): void { if (lDrag && !lDrag.seek) updateLoop(`loop ${fmtBars(LOOP[0])} – ${fmtBars(LOOP[1])}`); lDrag = null; }
function loopKey(e: KeyboardEvent): void {
  const d = (e.key === "ArrowRight" ? 1 : e.key === "ArrowLeft" ? -1 : 0) * (loopSnap() || 1); if (!d) return; e.preventDefault();
  if (e.shiftKey) LOOP[1] = Math.max(LOOP[0] + (loopSnap() || .25), LOOP[1] + d); else if (LOOP[0] + d >= 1) { LOOP[0] += d; LOOP[1] += d; }
  updateLoop(`loop ${fmtBars(LOOP[0])} – ${fmtBars(LOOP[1])}`);
}
/** Loop field: "start – end" in bars; false (and a hint) when invalid. */
export function onLoopText(str: string): boolean {
  const m = str.match(/(\d+(?:\.\d+)?)\s*[–\-—,: ]+\s*(\d+(?:\.\d+)?)/);
  if (!m || +m[1] < 1 || +m[2] <= +m[1]) { say("loop: type start – end in bars, e.g. 5 – 13"); return false; }
  LOOP[0] = +m[1]; LOOP[1] = +m[2]; setLoopOn(true); updateLoop(`loop ${fmtBars(LOOP[0])} – ${fmtBars(LOOP[1])}`); return true;
}
export const toggleLoop = (): void => { setLoopOn(!loopOn); updateLoop(loopOn ? "loop on" : "loop off"); };
export const loopSelection = (): void => { const cs = selClips(); if (!cs.length) { say("select clips to loop them"); return; }
  LOOP[0] = Math.min(...cs.map(x => x.c.start)); LOOP[1] = Math.max(...cs.map(x => x.c.start + x.c.len)); setLoopOn(true); updateLoop(`loop set to the selection ${fmtBars(LOOP[0])} – ${fmtBars(LOOP[1])}`); };
// Follow pauses when the user scrolls or zooms a view by hand (Ableton's rule); ⇥/F, play or a ruler click resume it.
export function toggleFollow(): void {
  if (followHold) { resumeFollow(); tr.setFollow(true); say("follow resumed"); showPos(); return; }
  tr.setFollow(!tr.follow()); resumeFollow();
}
/** A hand scroll or zoom over these pauses follow. */
export const holdOn = (els: HTMLElement[]): void => { for (const el of els) { el.addEventListener("wheel", holdFollow, { passive: true }); el.addEventListener("touchmove", holdFollow, { passive: true }); } };
// Dragging a scrollbar: the press lands on the scroll container itself, not on its content.
export const holdOnBar = (el: HTMLElement): void => el.addEventListener("pointerdown", e => { if (e.target === el) holdFollow(); });

// Runs every animation frame while playing: reads layout first (still valid from the last frame), then writes
// only what changed; the playheads move by transform so no frame re-lays-out the arrangement.
export function showPos(): void {
  liveState.setPos(SONG_POS);
  const x = (SONG_POS - 1) * BW;
  if (playing && tr.follow()) {
    const sl = lw.scrollLeft, cw = lw.clientWidth;
    // paused by a hand scroll: resumes by itself when the playhead, still in view, reaches the right edge
    if (followHold && x > sl + cw - 40 && x <= sl + cw) resumeFollow();
    if (!followHold && (x < sl || x > sl + cw - 40)) lw.scrollLeft = Math.max(0, x - 40);
    if (!followHold) hooks.rollFollow();
  }
  ph.style.transform = ph2.style.transform = `translateX(${x}px)`;
  hooks.rollPos();
}
let lastLane = 0;
/** While playing (10/s): lane value readouts and the playing clips' highlight. */
export function laneTick(now: number): void { if (now - lastLane < 100) return; lastLane = now; laneVals.forEach(l => l.val());
  for (const t of tracks) laneEl.get(t.id)?.querySelectorAll(".clip").forEach((el) => { const c = (el as HTMLElement & { _clip: UIClip })._clip; el.classList.toggle("playing", SONG_POS >= c.start && SONG_POS < c.start + c.len); }); }
/** Arrangement zoom by `f`, anchored at `clientX` (default: the view centre). */
export function zoomArr(f: number, clientX?: number, quiet?: boolean): void {
  const r = lw.getBoundingClientRect(), ax = (clientX ?? r.left + lw.clientWidth / 2) - r.left, s0 = lw.scrollLeft, b0 = BW;
  const nb = clamp(BW * f, 6, 400); if (Math.abs(nb - BW) < .01) return;
  setBW(nb); arrange.setBw(BW); document.documentElement.style.setProperty("--bar-w", BW + "px");
  relayout(); tracks.forEach((t) => arrange.bumpLane(t.id)); updateLoop(); drawCursor(); showPos();
  lw.scrollLeft = anchorScroll(s0, ax, b0, BW); if (!quiet) say(`arrangement zoom ${Math.round(BW / 44 * 100)}%`);
}
/** Shift+Z: the selected clips, else the song. */
export function fitArr(): void {
  const cs = selClips(), a = cs.length ? Math.min(...cs.map(x => x.c.start)) : 1, b1 = cs.length ? Math.max(...cs.map(x => x.c.start + x.c.len)) : BARS + 1;
  zoomArr((lw.clientWidth - 8) / (b1 - a) / BW, undefined, true); lw.scrollLeft = (a - 1) * BW; say(cs.length ? `zoomed to ${cs.length} clips` : "zoomed to the song");
}
