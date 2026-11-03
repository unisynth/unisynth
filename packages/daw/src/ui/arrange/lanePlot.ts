// Which automation points a lane draws (daw-solid-ts 9.5; the lane draws `laneLine` and `lanePlot(...).handles`). A converted song can hold thousands of points per lane,
// many per pixel: the line keeps the first, lowest, highest and last point of each pixel column, the handles (one
// focusable element each) the lowest and highest of each `handlePx` column, only for the bars in `[w0, w1]`; the
// selected point always keeps its handle. Points are `[bar, value]`, sorted by bar.

/** First index whose bar is ≥ `b` (points sorted by bar). */
function lowerBound(pts: readonly (readonly [number, number, number?])[], b: number): number {
  let lo = 0, hi = pts.length;
  while (lo < hi) { const m = (lo + hi) >> 1; if (pts[m]![0] < b) lo = m + 1; else hi = m; }
  return lo;
}

/** Point indices for the whole line, thinned per pixel column, ascending. */
export function laneLine(pts: readonly (readonly [number, number, number?])[], x: (bar: number) => number): number[] {
  return pts.length ? thin(pts, 0, pts.length - 1, x, 1, true) : [];
}

/** Point indices for the line (incl. one neighbour outside the window on each side) and for the handles, ascending. */
export function lanePlot(pts: readonly (readonly [number, number, number?])[], w0: number, w1: number, x: (bar: number) => number,
  handlePx: number, sel: number): { line: number[]; handles: number[] } {
  const n = pts.length;
  if (!n) return { line: [], handles: [] };
  const a = lowerBound(pts, w0), z = lowerBound(pts, w1 + 1e-9) - 1;  // [a, z]: points in the window
  const line = thin(pts, Math.max(0, a - 1), Math.min(n - 1, z + 1), x, 1, true);
  const handles = a <= z ? thin(pts, a, z, x, handlePx, false) : [];
  if (sel >= 0 && sel < n && !handles.includes(sel)) { handles.push(sel); handles.sort((p, q) => p - q); }
  return { line, handles };
}

/** Per column of `px` pixels: the lowest and highest point (and the first and last with `ends`), in index order. */
function thin(pts: readonly (readonly [number, number, number?])[], from: number, to: number, x: (bar: number) => number, px: number, ends: boolean): number[] {
  const out: number[] = [];
  let col = NaN, first = -1, lo = -1, hi = -1, last = -1;
  const flush = () => { if (first < 0) return; const k = ends ? [first, lo, hi, last] : [lo, hi]; for (const i of [...new Set(k)].sort((p, q) => p - q)) out.push(i); };
  for (let i = from; i <= to; i++) {
    const c = Math.floor(x(pts[i]![0]) / px), v = pts[i]![1];
    if (c !== col) { flush(); col = c; first = lo = hi = i; }
    if (v < pts[lo]![1]) lo = i;
    if (v > pts[hi]![1]) hi = i;
    last = i;
  }
  flush();
  return out;
}
