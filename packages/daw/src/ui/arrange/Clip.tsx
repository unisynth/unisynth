// A clip in a track lane (change daw-solid-ts 5.3): position, size, label and state are reactive on the clip (song
// store); the note thumbnail is one canvas drawn imperatively (design D4, `thumb.ts`) when the lane's content version
// (`laneVer`, bumped by renderLane), the clip's length/loop/name or the zoom change. Same classes, role and ARIA as
// the old clip; the element carries `_clip` for the lane's pointer handlers in app.js.
import { createEffect, createMemo, onCleanup, type JSX } from "solid-js";
import { bw, laneVer } from "../state/arrange.ts";
import { thumb, type ThumbNote } from "./thumb.ts";

export interface ClipModel { name: string; start: number; len: number; loop?: boolean; sel?: boolean }
export interface ClipTrack { id: string; col: string; bus?: boolean; clips: ClipModel[] }
/** The clip's pattern, read without creating it (no store writes from a draw). */
export type PatternOf = (t: ClipTrack, name: string) => { len: number; notes: readonly ThumbNote[] } | null;

// .clip: top/bottom 3 px, border 1 px; .notes: inset 15px 2px 2px 2px (style.css); lane height --track-h 56
const NOTES_H = 56 - 1 - 6 - 2 - 17, MAX_W = 8192;

export function Clip(p: { t: ClipTrack; c: ClipModel; pattern: PatternOf; later: (draw: () => void) => () => void }): JSX.Element {
  const t = p.t, c = p.c;
  const links = createMemo(() => t.clips.filter((x) => x.name === c.name).length);
  const label = () => `Clip ${c.name}, bar ${+c.start.toFixed(2)}, ${+c.len.toFixed(2)} bars${c.loop ? ", looping" : ""}${links() > 1 ? ", linked" : ""}${c.sel ? ", selected" : ""}`;
  let cv!: HTMLCanvasElement;
  createEffect(() => {
    laneVer[t.id];  // content changed (renderLane)
    const w = Math.max(1, c.len * bw() - 7), loop = !!c.loop, len = c.len, name = c.name;
    const draw = () => {
      const pt = t.bus ? null : p.pattern(t, name), dpr = Math.min(2, devicePixelRatio || 1), sx = Math.min(1, MAX_W / (w * dpr));
      cv.width = Math.ceil(w * dpr * sx); cv.height = Math.ceil(NOTES_H * dpr); cv.style.width = w + "px"; cv.style.height = NOTES_H + "px";
      const g = cv.getContext("2d")!; g.setTransform(dpr * sx, 0, 0, dpr, 0, 0);
      const r = thumb(pt?.notes ?? [], { len, loop, patLen: pt?.len ?? len, w, h: NOTES_H });
      g.fillStyle = "#0d0f12b0";
      for (const n of r.notes) g.fillRect(n.x, n.y, n.w, 2);
      if (r.marks.length) { g.strokeStyle = "#0009"; g.setLineDash([3, 2]); g.beginPath(); for (const x of r.marks) { g.moveTo(Math.round(x) + 0.5, 0); g.lineTo(Math.round(x) + 0.5, NOTES_H); } g.stroke(); }
    };
    onCleanup(p.later(draw));
  });
  return (
    <div class="clip" classList={{ linked: links() > 1, sel: !!c.sel }} data-track={t.id} ref={(el) => ((el as any)._clip = c)}
      style={{ left: `${(c.start - 1) * bw()}px`, width: `${c.len * bw() - 1}px`, background: `var(${t.col})` }}
      tabIndex={-1} role="button" aria-label={label()}>
      <div class="lbl">{c.name}{c.loop ? " ⟳" : ""}</div>
      <canvas ref={cv} class="notes" aria-hidden="true" />
      <div class="rz" title="Drag: crop or extend · Alt+drag: loop the clip" />
    </div>
  );
}
