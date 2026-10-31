// Undo/redo history (change daw-solid-ts, design D2): JSON snapshots of the song (tracks + master) taken before a
// gesture; a commit with an unchanged snapshot is dropped. Each committed state gets an id; undo/redo return to the
// id they restore, so "unsaved" = current id ≠ id at save (no snapshot compare per edit: a snapshot of a large song
// costs 10–20 ms). The caller restores the returned snapshot (`restoreSong`) and refreshes its views.
interface Entry { js: string; id: number }

export interface History {
  /** Records `before` as an undo step when it differs from `after`; true if recorded. Clears redo. */
  commit(before: string, after: string): boolean;
  /** The snapshot to restore for undo (`now` goes to redo), or null when there is nothing to undo. */
  undo(now: string): string | null;
  redo(now: string): string | null;
  /** Forgets every step (a song was loaded); the current state gets a fresh id. */
  clear(): void;
  readonly depth: number;
  readonly canRedo: boolean;
  /** Id of the current state; equal ids = same committed state. */
  readonly stateId: number;
}

export function createHistory(): History {
  const undoStack: Entry[] = [], redoStack: Entry[] = [];
  let cur = 0, next = 1;
  const move = (from: Entry[], to: Entry[], now: string) => {
    const e = from.pop();
    if (!e) return null;
    to.push({ js: now, id: cur });
    cur = e.id;
    return e.js;
  };
  return {
    commit(before, after) {
      if (before === after) return false;
      undoStack.push({ js: before, id: cur });
      redoStack.length = 0;
      cur = next++;
      return true;
    },
    undo: (now) => move(undoStack, redoStack, now),
    redo: (now) => move(redoStack, undoStack, now),
    clear() { undoStack.length = redoStack.length = 0; cur = next++; },
    get depth() { return undoStack.length; },
    get canRedo() { return redoStack.length > 0; },
    get stateId() { return cur; },
  };
}
