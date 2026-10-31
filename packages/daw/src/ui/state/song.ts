// The song store (change daw-solid-ts, design D2). Transition phase: a `createMutable` proxy over the editor model,
// so app.js's in-place writes keep working and now notify Solid components. Bulk readers (export, undo snapshots,
// structuredClone, canvas loops) take `raw()` — the plain objects, no proxy cost, never tracked. When the write
// ratchet (task 2.7) reaches 0 this becomes `createStore` with actions.
import { batch } from "solid-js";
import { $RAW, createMutable, modifyMutable, reconcile } from "solid-js/store";
import type { UITrack } from "../../model.ts";

export interface SongRoot { tracks: UITrack[]; master: UITrack }

/** Wraps the loaded model; `tracks` and `master` of the result are the proxies app.js mutates from now on. */
export function createSong(st: SongRoot): SongRoot {
  return createMutable({ tracks: st.tracks, master: st.master });
}

/** The plain object behind a store proxy, in O(1); a non-proxy is returned as is. (Solid's `unwrap` deep-walks a
 * non-proxy object looking for nested proxies — far too slow for hot read paths.) */
export const raw = <T>(x: T): T => ((x as any)?.[$RAW] ?? x) as T;

/** Runs a multi-write edit as one update (one notification pass instead of one per proxy write). */
export { batch };

/** Restores a snapshot into the song: `tracks` reconciled by track id, then `master` — never the root (a root
 * reconcile deletes every key the snapshot lacks). Unchanged tracks, devices and values keep their identity; arrays
 * without ids (clips, notes, sends, lane points) merge by position (values exact, identity by index). */
export function restoreSong(song: SongRoot, d: SongRoot): void {
  batch(() => {
    modifyMutable(song.tracks, reconcile(d.tracks, { key: "id" }));
    modifyMutable(song.master, reconcile(d.master, { key: "id" }));
  });
}
