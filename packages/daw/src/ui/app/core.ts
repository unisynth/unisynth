// Edit core (change daw-solid-ts 9.1, step 2): status messages, and undo as one whole-song snapshot per gesture or
// command (tracks, clips, notes, devices, automation, mixer, selection). `initCore` takes what a restore and a
// commit must refresh in the rest of the UI.
import { createHistory } from "../state/history.ts";
import { batch, raw, restoreSong } from "../state/song.ts";
import { overrides, rollClips } from "../state/view.ts";
import { armed, master, selected, setArmed, setSelected, song, tracks } from "./session.ts";

export interface CoreHooks {
  /** After a restore: views rebuilt for the restored song and selection. */
  restored(): void;
  /** After any committed change (and restore): unsaved state and the engine follow. */
  changed(): void;
}
let hooks: CoreHooks = { restored() {}, changed() {} };
let statusEl: HTMLElement | null = null, liveEl: HTMLElement | null = null;

/** Undo/redo history with state ids (ui/state/history.ts). */
export const H = createHistory();

export function initCore(h: CoreHooks): void {
  hooks = h;
  if (typeof document !== "undefined") { statusEl = document.getElementById("edit-status"); liveEl = document.getElementById("live"); }
}
/** Shows a status message and announces it (aria-live). */
export function say(m: string): void { if (statusEl) statusEl.textContent = m; if (liveEl) liveEl.textContent = m; }
/** The last status message (commands built from several steps extend it). */
export const lastSaid = (): string => statusEl?.textContent ?? "";

// No replacer: view fields live in ui/state/view.ts, and a replacer function runs per key (212 ms per knob step on a
// 4 MB song vs ~11 ms native, bench/ui-profile.ts).
export const snap = (): string => JSON.stringify({ tracks: raw(tracks), master: raw(master), sel: selected.id, armed: armed?.id });

/** Puts a snapshot back: one notification pass for the whole restore. */
export function restore(js: string): void { batch(() => restore1(js)); }
function restore1(js: string): void {
  const d = JSON.parse(js);
  overrides.clear();  // restored tracks follow their automation lanes again
  // the roll's clip by name/start (restored clips merge by position: an object reference could now hold another
  // clip's values)
  for (const [id, c] of rollClips) if (c) { const r = c as { name: string; start: number }; rollClips.set(id, { name: r.name, start: r.start }); }
  restoreSong(song, d);  // tracks by id, then master; unchanged objects keep identity
  setSelected(tracks.find((t) => t.id === d.sel) || (d.sel === "master" ? master : tracks[0]!));
  setArmed(tracks.find((t) => t.id === d.armed) || null);
  hooks.restored(); hooks.changed();
}
/** One undo step from `before` to now, unless nothing changed; says `msg`. */
export function commit(before: string, msg: string): void { if (!H.commit(before, snap())) return; say(msg); hooks.changed(); }
export function undo(): void { const js = H.undo(snap()); if (js == null) return say("nothing to undo"); restore(js); say("undo"); }
export function redo(): void { const js = H.redo(snap()); if (js == null) return say("nothing to redo"); restore(js); say("redo"); }
