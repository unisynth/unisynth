// Display state that changes at frame rate (change daw-solid-ts, design D3): playhead position, transport, engine
// load and meter levels. Written from the engine status / rAF loop in app.js, read by components. Never part of the
// song store: not saved, not undoable, never pushed to the engine. Setters notify only on a shown change.
// (Automation values at the playhead stay in app.js `autoVal` until the device panel is ported, task 6.2.)
import { createSignal } from "solid-js";
import { createStore } from "solid-js/store";

/** Playhead position in bars (1-based). */
export const [pos, setPos] = createSignal(1);
export const [playing, setPlaying] = createSignal(false);
/** Engine load as shown, percent (integer). */
export const [load, setLoad] = createSignal(0);

/** Meter levels (0..1, [L, R]) by track id, as shown. */
export const [peaks, setPeaks] = createStore<Record<string, [number, number]>>({});
/** Smallest meter change that is shown (the meter bar is ~100 px tall). */
export const PEAK_STEP = 2e-3;

/** Publishes a track's meter levels when either channel moved by at least `PEAK_STEP`. */
export function setPeak(id: string, l: number, r: number): void {
  const sh = peaks[id];
  if (sh && Math.abs(sh[0] - l) < PEAK_STEP && Math.abs(sh[1] - r) < PEAK_STEP) return;
  setPeaks(id, [l, r]);
}
