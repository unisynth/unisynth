// Arrangement view state (change daw-solid-ts 5.2, design D6): windowing of track rows and the header state that is
// not in the song (armed, focused, renaming). Row heights come from the model (fixed CSS heights, border-box): a track
// row is --track-h, plus its automation lanes and the "+ lane" row while open. Rows meeting the view plus one screen
// above and below exist; the selected and the focused row always exist; spacers keep offsets and the scroll extent.
import { createSignal } from "solid-js";
import { createStore } from "solid-js/store";

export const TRACK_H = 56, AUTO_H = 40, ADD_H = 24;
export const rowHeight = (open: boolean, lanes: number) => TRACK_H + (open ? lanes * AUTO_H + ADD_H : 0);

/** Indices of the rows to render: those meeting [top − view, top + 2·view), plus `pinned`, ascending. */
export function windowRows(heights: number[], top: number, view: number, pinned: number[]): number[] {
  const lo = top - view, hi = top + 2 * view, out = new Set<number>();
  let y = 0;
  for (let i = 0; i < heights.length; i++) { const y1 = y + heights[i]; if (y1 > lo && y < hi) out.add(i); else if (y >= hi) break; y = y1; }
  for (const i of pinned) if (i >= 0 && i < heights.length) out.add(i);
  return [...out].sort((a, b) => a - b);
}

/** Rows (ascending) with spacer gaps between them: row indices and { gap: px }. */
export function segments(heights: number[], rows: number[]): (number | { gap: number })[] {
  const out: (number | { gap: number })[] = [];
  let next = 0, gap = 0;
  for (const i of rows) {
    for (; next < i; next++) gap += heights[next];
    if (gap) out.push({ gap }); gap = 0;
    out.push(i); next = i + 1;
  }
  for (; next < heights.length; next++) gap += heights[next];
  if (gap) out.push({ gap });
  return out;
}

// ---- signals (module scope: a component hot reload keeps them) ----
/** Scroll position and height of the lanes scroller. */
export const [scrollTop, setScrollTop] = createSignal(0);
export const [viewH, setViewH] = createSignal(600);
export const [armedId, setArmedId] = createSignal<string | null>(null);
export const [focusedId, setFocusedId] = createSignal<string | null>(null);
export const [renamingId, setRenamingId] = createSignal<string | null>(null);
/** Bumped by rebuildTracks(): rows refill their imperative content (clips, automation lanes). */
export const [rev, setRev] = createSignal(0);
/** Pixels per bar (arrangement zoom) and the horizontal scroll: clips are culled to the bars in view ± one screen. */
export const [bw, setBw] = createSignal(44);
export const [scrollLeft, setScrollLeft] = createSignal(0);
export const [viewW, setViewW] = createSignal(1000);
/** Bars a lane renders clips for: whole view-width pages around the view (changes only when a page boundary is
 *  crossed), ± one page. */
export function barWindow(left: number, view: number, pxPerBar: number): [number, number] {
  const page = Math.floor(left / Math.max(1, view));
  return [1 + ((page - 1) * view) / pxPerBar, 1 + ((page + 3) * view) / pxPerBar];
}
/** Per-track content version: renderLane(t) bumps it, the lane's clip thumbnails redraw. */
export const [laneVer, setLaneVer] = createStore<Record<string, number>>({});
export const bumpLane = (id: string) => setLaneVer(id, (v) => (v ?? 0) + 1);
/** Bumped by renderAll(): device labels re-read fields written outside the store (e.g. `_edited`). */
export const [labelRev, setLabelRev] = createSignal(0);
