// Edit core (change daw-solid-ts 9.1, step 2): status messages, and undo as one song snapshot per gesture or command
// (tracks, clips, notes, devices, automation, mixer, selection; ui/state/snapshot.ts caches lane points, 9.3). Undo and
// redo restore only the tracks that differ from the current state while the track list is the same. `initCore` takes
// what a restore and a commit must refresh in the rest of the UI.
import { createHistory } from "../state/history.ts";
import { batch, raw, restoreSong, restoreTracks } from "../state/song.ts";
import { makeSnap, primeSnap, type Snap, type SnapMeta, snapEqual, snapState, trackSnapEqual, trackState } from "../state/snapshot.ts";
import type { UITrack } from "../../model.ts";
import { overrides, rollClips } from "../state/view.ts";
import { armed, master, selected, setArmed, setSelected, song, tracks } from "./session.ts";

export interface CoreHooks {
  /** After a restore: views rebuilt for the restored song and selection. `ids`: only these tracks changed (the track
   *  list is the same); absent: a full restore. */
  restored(ids?: readonly string[]): void;
  /** After any committed change (and restore): unsaved state and the engine follow. */
  changed(): void;
}
let hooks: CoreHooks = { restored() {}, changed() {} };
let statusEl: HTMLElement | null = null, liveEl: HTMLElement | null = null;

/** Undo/redo history with state ids (ui/state/history.ts). */
export const H = createHistory<Snap>(snapEqual);
/** What the last restore did (tests): "full" or "tracks <n>[ + master]". */
export const restoreStats = { last: "" };

export function initCore(h: CoreHooks): void {
  hooks = h;
  if (typeof document !== "undefined") { statusEl = document.getElementById("edit-status"); liveEl = document.getElementById("live"); }
}
/** Shows a status message and announces it (aria-live). */
export function say(m: string): void { if (statusEl) statusEl.textContent = m; if (liveEl) liveEl.textContent = m; }
/** Announces a message to screen readers only (aria-live), leaving the status line. */
export function announce(m: string): void { if (liveEl) liveEl.textContent = m; }
/** Text last announced (aria-live). */
export const announced = (): string => liveEl?.textContent ?? "";
/** The last status message (commands built from several steps extend it). */
export const lastSaid = (): string => statusEl?.textContent ?? "";

// No replacer: view fields live in ui/state/view.ts, and a replacer function runs per key (212 ms per knob step on a
// 4 MB song vs ~11 ms native, bench/ui-profile.ts).
export const snap = (): Snap => makeSnap(raw(tracks), raw(master), { sel: selected.id, armed: armed?.id }, H.epoch);

/** Puts a snapshot back: one notification pass. With `now` (the current state) and the same track list, only the
 * tracks (and master) that differ are parsed and reconciled; otherwise the whole song. */
export function restore(s: Snap, now?: Snap): void { batch(() => restore1(s, now)); }
/** Puts a freshly loaded song in place (a full restore; the caller clears the history). */
export function restoreState(st: { tracks: UITrack[]; master: UITrack } & SnapMeta): void {
  restore(makeSnap(st.tracks, st.master, { sel: st.sel, armed: st.armed }, H.epoch));
}
function restore1(s: Snap, now?: Snap): void {
  const d = JSON.parse(s.meta) as SnapMeta;
  overrides.clear();  // restored tracks follow their automation lanes again
  // the roll's clip by name/start (restored clips merge by position: an object reference could now hold another
  // clip's values)
  for (const [id, c] of rollClips) if (c) { const r = c as { name: string; start: number }; rollClips.set(id, { name: r.name, start: r.start }); }
  let changedIds: string[] | undefined;
  const same = !!now && now.master.id === s.master.id && now.tracks.length === s.tracks.length && now.tracks.every((t, i) => t.id === s.tracks[i]!.id);
  if (same) {
    const at = s.tracks.flatMap((ts, i): [number, UITrack][] => trackSnapEqual(ts, now!.tracks[i]!) ? [] : [[i, trackState(ts)]]);
    const m = trackSnapEqual(s.master, now!.master) ? undefined : trackState(s.master);
    restoreTracks(song, at, m);
    restoreStats.last = `tracks ${at.length}${m ? " + master" : ""}`;
    changedIds = [...at.map(([i]) => s.tracks[i]!.id), ...(m ? [s.master.id] : [])];
  } else {
    restoreSong(song, snapState(s));  // tracks by id, then master; unchanged objects keep identity
    restoreStats.last = "full"; changedIds = undefined;
  }
  primeSnap(raw(tracks), raw(master), s);  // the reconciled lane arrays now hold `s`'s points
  setSelected(tracks.find((t) => t.id === d.sel) || (d.sel === "master" ? master : tracks[0]!));
  setArmed(tracks.find((t) => t.id === d.armed) || null);
  hooks.restored(changedIds); hooks.changed();
}
/** One undo step from `before` to now, unless nothing changed or `before` was taken before the history was cleared
 * (a gesture across a song load); says `msg`. */
export function commit(before: Snap, msg: string): void { if (before.epoch !== H.epoch || !H.commit(before, snap())) return; say(msg); hooks.changed(); }
export function undo(): void { const now = snap(), s = H.undo(now); if (s == null) return say("nothing to undo"); restore(s, now); say("undo"); }
export function redo(): void { const now = snap(), s = H.redo(now); if (s == null) return say("nothing to redo"); restore(s, now); say("redo"); }
