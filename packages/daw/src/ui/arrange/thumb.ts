// Clip thumbnail geometry (change daw-solid-ts 5.3, design D4): where the old clip drew one <i> per note, as
// rectangles for one canvas per clip. A looping clip repeats its pattern (pattern length `patLen` bars) with a mark
// at each repeat; a cropped clip shows the pattern's start. Steps are 1/16 bar. Pure: unit-tested.
export interface ThumbNote { s: number; l: number; n: number }
export interface ThumbOpts { len: number; loop: boolean; patLen: number; w: number; h: number }

export function thumb(notes: readonly ThumbNote[], o: ThumbOpts): { notes: { x: number; y: number; w: number }[]; marks: number[] } {
  const steps = o.len * 16, per = o.loop ? o.patLen * 16 : Infinity, marks: number[] = [], out: { x: number; y: number; w: number }[] = [];
  if (o.loop) for (let x = per; x < steps; x += per) marks.push((x / steps) * o.w);
  if (!notes.length) return { notes: out, marks };
  let lo = Infinity, hi = -Infinity;
  for (const n of notes) { if (n.n < lo) lo = n.n; if (n.n > hi) hi = n.n; }
  const span = Math.max(1, hi - lo);
  for (let k = 0; k === 0 || (o.loop && k * per < steps); k++) for (const n of notes) {
    if (o.loop && n.s >= per) continue;
    const s0 = n.s + k * (o.loop ? per : 0); if (s0 >= steps) continue;
    out.push({ x: (s0 / steps) * o.w, y: ((hi - n.n) / span) * 0.88 * o.h, w: Math.max(1.5, (Math.min(n.l, steps - s0) / steps) * o.w) });
  }
  return { notes: out, marks };
}
