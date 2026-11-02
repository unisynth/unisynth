// Undo snapshots (change daw-solid-ts 9.3, design D2 "Snapshot cost"). A snapshot holds, per track (and the master),
// the track serialized without its automation lanes, the lanes without their points, and one JSON string per lane's
// points. Lane points are most of a converted song (6.10 of 6.57 MB on tbl-stash-over2), so their strings are cached
// per points array: a gesture re-serializes the track bodies and only the lanes it replaced.
//
// Invariant: a lane's `pts` array is never changed in place — an edit assigns a new array (`ln.pts = [...]`). Verify
// mode (`setVerify`, on in automated browser tests) re-serializes every cached lane and throws when it differs.
// `restoreSong` reconciles into the existing arrays (same identity, new contents), so it drops the cache; the caller
// re-primes it from the snapshot it restored (`primeSnap`).
import type { UITrack } from "../../model.ts";

type Lane = { pts?: unknown[] } & Record<string, unknown>;
export interface TrackSnap { id: string; body: string; lanes: string; pts: string[] }
/** One undoable state. `epoch`: the history generation it was taken in (a commit across a song load is dropped). */
export interface Snap { epoch: number; meta: string; tracks: TrackSnap[]; master: TrackSnap }
/** Selection and arming, restored with the song. */
export interface SnapMeta { sel?: string; armed?: string }

let cache = new WeakMap<object, string>();
let verify = false;
/** Counters for tests: lane point arrays serialized (cache misses). */
export const snapStats = { pts: 0 };
export function setVerify(on: boolean): void { verify = on; }
/** Forgets every cached lane (restoreSong: reconciled arrays keep their identity with new contents). */
export function resetSnapCache(): void { cache = new WeakMap(); }

function ptsJson(pts: unknown[] | undefined): string {
  if (!Array.isArray(pts)) return "null";
  let s = cache.get(pts);
  if (s === undefined) { s = JSON.stringify(pts); cache.set(pts, s); snapStats.pts++; }
  else if (verify && JSON.stringify(pts) !== s) throw new Error("undo snapshot: automation points were changed in place; assign a new array (ln.pts = [...])");
  return s;
}
/** One raw track (store-unwrapped): body without lanes, lane headers without points, cached points. */
export function trackSnap(t: UITrack): TrackSnap {
  const { lanes, ...body } = t as unknown as { lanes?: Lane[] } & Record<string, unknown>, ls = lanes ?? [];
  return { id: t.id, body: JSON.stringify(body), lanes: JSON.stringify(ls.map(({ pts, ...h }) => h)), pts: ls.map((l) => ptsJson(l.pts)) };
}
export function makeSnap(tracks: UITrack[], master: UITrack, meta: SnapMeta, epoch: number): Snap {
  return { epoch, meta: JSON.stringify(meta), tracks: tracks.map(trackSnap), master: trackSnap(master) };
}

export function trackSnapEqual(a: TrackSnap, b: TrackSnap): boolean {
  if (a.id !== b.id || a.body !== b.body || a.lanes !== b.lanes || a.pts.length !== b.pts.length) return false;
  for (let i = 0; i < a.pts.length; i++) if (a.pts[i] !== b.pts[i]) return false;
  return true;
}
/** Same song state (epochs are not compared: the caller decides what an old epoch means). */
export function snapEqual(a: Snap, b: Snap): boolean {
  if (a.meta !== b.meta || a.tracks.length !== b.tracks.length || !trackSnapEqual(a.master, b.master)) return false;
  for (let i = 0; i < a.tracks.length; i++) if (!trackSnapEqual(a.tracks[i]!, b.tracks[i]!)) return false;
  return true;
}

/** The track object a TrackSnap stands for (fresh objects; lanes get their points back). */
export function trackState(s: TrackSnap): UITrack {
  const t = JSON.parse(s.body), lanes = JSON.parse(s.lanes) as Lane[];
  t.lanes = lanes.map((h, i) => { const p = JSON.parse(s.pts[i]!); return p == null ? h : { ...h, pts: p }; });
  return t as UITrack;
}
export function snapState(s: Snap): { tracks: UITrack[]; master: UITrack } & SnapMeta {
  return { ...(JSON.parse(s.meta) as SnapMeta), tracks: s.tracks.map(trackState), master: trackState(s.master) };
}

/** After restoring `s` into `tracks` / `master` (raw, same order as `s`): their lane arrays hold `s`'s points. */
export function primeSnap(tracks: UITrack[], master: UITrack, s: Snap): void {
  const prime = (t: UITrack, ts: TrackSnap): void => {
    const ls = (t as unknown as { lanes?: Lane[] }).lanes ?? [];
    if (t.id !== ts.id || ls.length !== ts.pts.length) return;
    ls.forEach((l, i) => { if (Array.isArray(l.pts)) cache.set(l.pts, ts.pts[i]!); });
  };
  tracks.forEach((t, i) => { const ts = s.tracks[i]; if (ts) prime(t, ts); });
  prime(master, s.master);
}
