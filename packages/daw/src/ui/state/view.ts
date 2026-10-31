// Editor view state kept OFF the song objects, keyed by track id (change daw-solid-ts, design D2): the song store's
// undo reconciles track objects and would delete fields it does not snapshot, and DOM nodes do not belong in a store.
// Not saved, not undoable.

/** Arrangement lane element of each track (set when the lane is built). */
export const laneEl = new Map<string, HTMLElement>();
/** Automation keys (`<device index>:<param>`) the user turned by hand: those knobs stop following their lane. */
export const overrides = new Map<string, Set<string>>();
/** Clip each track last opened in the piano roll (object as loaded; resolved again by name/start after an undo). */
export const rollClips = new Map<string, unknown>();

/** The override set of a track, created on first use. */
export function overrideSet(id: string): Set<string> {
  let s = overrides.get(id);
  if (!s) overrides.set(id, (s = new Set()));
  return s;
}
