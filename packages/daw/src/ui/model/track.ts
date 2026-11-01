// Track helpers on the song model (change daw-solid-ts 9.1, step 1): devices of a track, its patterns, lane text.
// Pure apart from `patOf`, which creates a missing pattern (a model write, like before the move).
import type { Device, UIPattern, UITrack } from "../../model.ts";
import { raw } from "../state/song.ts";
import { fmtVal, paramOf, tableOf } from "./params.ts";

type Lane = UITrack["lanes"][number];
/** A device as the UI holds it: `_edited` marks a preset changed since it was loaded (undoable, never saved). */
export type UIDevice = Device & { _edited?: boolean; _dev?: unknown };

export const instOf = (t: UITrack): Device | undefined => t.devices.find((d) => d.kind === 64);
export const fxOf = (t: UITrack): Device[] => t.devices.filter((d) => d.kind !== 64);
export const devicesOf = (t: UITrack): string[] => t.devices.map((d) => d.kind === 64 ? "Unisynth" : tableOf(d)?.label ?? `kind ${d.kind}`);
/** Placements sharing a clip name (linked clips). */
export const links = (t: UITrack, name: string): number => t.clips.filter((c) => c.name === name).length;
/** Index of a device among the ones the engine runs (bypassed devices are left out); -1 when bypassed. */
export const engineDev = (t: UITrack, d: Device): number => t.devices.filter((x) => !x.bypass).indexOf(d);
/** A device without its UI-only fields (for saving and undo snapshots). */
export const cleanDev = (d: UIDevice): Device => { const { _edited, _dev, ...x } = d; return x; };
/** Text of an automation lane value: the parameter's own text, else percent. */
export const laneText = (t: UITrack, ln: Lane, v: number): string => { const d = t.devices[ln.dev], p = d && paramOf(d, ln.param); return d && p ? fmtVal(d, p, v) : (v * 100).toFixed(0) + "%"; };

/** Notes live in a pattern per clip name: linked placements share it, a shorter placement plays its start. Creates a
 * missing pattern (as long as the longest placement). */
export function patOf(t: UITrack, name: string): UIPattern & { cc: NonNullable<UIPattern["cc"]> } {
  t.pats ??= {};
  if (!t.pats[name]) t.pats[name] = { len: Math.max(1, Math.round(Math.max(...t.clips.filter((c) => c.name === name).map((c) => c.len), 1))), notes: [], cc: { wheel: [], bend: [] } };
  const pt = t.pats[name]!; pt.cc ??= { wheel: [], bend: [] }; return pt as UIPattern & { cc: NonNullable<UIPattern["cc"]> };
}
/** A clip moved or pasted onto another track brings its notes (never overwrites a pattern already there). */
export const carry = (from: UITrack, to: UITrack, name: string, pat?: UIPattern): void => {
  if (from !== to && !to.pats?.[name]) (to.pats ??= {})[name] = structuredClone(raw(pat || patOf(from, name)));
};
