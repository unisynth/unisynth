// Files state (change daw-solid-ts 4.2): the songs in project/music, the open song, its unsaved-change count. ui/app/files.ts
// publishes them (list at start and after a save, file on open, dirty in dirtyUpd); the song list and the Save button
// render from here.
import { createSignal } from "solid-js";

export const [songs, setSongs] = createSignal<string[]>([]);
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
