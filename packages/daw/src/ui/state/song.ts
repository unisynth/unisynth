// The song store (change daw-solid-ts, design D2). Transition phase: a `createMutable` proxy over the editor model,
// so the session modules' in-place writes keep working and now notify Solid components. Bulk readers (export, undo snapshots,
// structuredClone, canvas loops) take `raw()` — the plain objects, no proxy cost, never tracked. When the write
// ratchet (task 2.7) reaches 0 this becomes `createStore` with actions.
import { batch } from "solid-js";
import { $RAW, createMutable, modifyMutable, reconcile } from "solid-js/store";
import { resetSnapCache } from "./snapshot.ts";
import type { UITrack } from "../../model.ts";

export interface SongRoot { tracks: UITrack[]; master: UITrack }

/** Wraps the loaded model; `tracks` and `master` of the result are the proxies the session modules (ui/app/) mutate from now on. */
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
 * without ids (clips, notes, sends, lane points) merge by position (values exact, identity by index), so the undo
 * snapshot cache of lane points is dropped (ui/state/snapshot.ts). */
export function restoreSong(song: SongRoot, d: SongRoot): void {
  resetSnapCache();
  batch(() => {
    modifyMutable(song.tracks, reconcile(d.tracks, { key: "id" }));
    modifyMutable(song.master, reconcile(d.master, { key: "id" }));
  });
}
/** Restores only some tracks (by index; the track list itself unchanged) and optionally the master, each reconciled
 * by id like `restoreSong` (undo of a one-track edit does not parse or reconcile the other tracks). */
export function restoreTracks(song: SongRoot, at: [number, UITrack][], master?: UITrack): void {
  resetSnapCache();
  batch(() => {
    for (const [i, t] of at) modifyMutable(song.tracks[i]!, reconcile(t, { key: "id" }));
    if (master) modifyMutable(song.master, reconcile(master, { key: "id" }));
  });
}

/** A device value edited on the panel, with `extra` values set in the same edit (change add-fudge-softsynth: a slot
 * that becomes an engine type takes that type's shape values). An action: the write ratchet's way out of the session
 * modules. */
export function setDeviceValue(d: { values: Record<string, number> }, name: string, v: number, extra: Record<string, number>): void {
  batch(() => { d.values[name] = v; Object.assign(d.values, extra); });
}
