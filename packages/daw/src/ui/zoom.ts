// Anchored zoom (change daw-solid-ts 8.2): the arrangement (px per bar) and the piano roll (px per 1/16, px per key
// row) zoom about a point, so the content under the pointer (or the view's centre) stays in place.
export const clamp = (v: number, a: number, b: number): number => Math.min(b, Math.max(a, v));
/** Zoom factor of a wheel event: pinch ≈ small deltas, a mouse notch ≈ ±100 (line mode ×33). */
export const wheelF = (e: { deltaY: number; deltaMode: number }): number => Math.exp(-clamp(e.deltaY * (e.deltaMode === 1 ? 33 : 1), -60, 60) * .008);
/** Scroll position keeping the content at `anchor` px (from the view's edge) in place when a unit goes `u0` -> `u1`. */
export const anchorScroll = (scroll: number, anchor: number, u0: number, u1: number): number => (scroll + anchor) / u0 * u1 - anchor;
