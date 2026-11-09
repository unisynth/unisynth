// Files state (change daw-solid-ts 4.2): the songs in project/music, the open song, its unsaved-change count. ui/app/files.ts
// publishes them (list at start and after a save, file on open, dirty in dirtyUpd); the song list and the Save button
// render from here.
import { createSignal } from "solid-js";
import type { SongInfo } from "../../model.ts";

export const [songs, setSongs] = createSignal<string[]>([]);
/** Title, author and length per song path (server list, then the saved or opened song). */
export const [infos, setInfos] = createSignal<Record<string, SongInfo>>({});
export const setInfo = (path: string, i: SongInfo) => setInfos({ ...infos(), [path]: i });
/** The song-list search text. */
export const [query, setQuery] = createSignal("");
export const [songFile, setSongFile] = createSignal("");
/** Unsaved changes: `on` (dirty) and how many undo steps away from the saved state. */
export const [dirty, setDirty] = createSignal({ on: false, n: 0 }, { equals: (a, b) => a.on === b.on && a.n === b.n });

/** Adds a path to the song list (kept sorted, no duplicates). */
export function addSong(path: string): void {
  if (!songs().includes(path)) setSongs([...songs(), path].sort());
}
export const base = (p: string) => p.split("/").pop()!;
/** Save button label for screen readers. */
export const saveLabel = (file: string, d: { on: boolean; n: number }) =>
  d.on ? `Save ${base(file)}, ${d.n} unsaved change${d.n > 1 ? "s" : ""}` : `Save ${base(file)}, no changes`;
/** "m:ss" (or "h:mm:ss") of a length in seconds. */
export function fmtLen(sec: number): string {
  const t = Math.round(sec), h = Math.floor(t / 3600), m = Math.floor(t / 60) % 60, ss = String(t % 60).padStart(2, "0");
  return h ? `${h}:${String(m).padStart(2, "0")}:${ss}` : `${m}:${ss}`;
}
