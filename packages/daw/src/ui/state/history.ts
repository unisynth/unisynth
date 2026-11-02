// Undo/redo history (change daw-solid-ts, design D2): snapshots of the song taken before a gesture (any type with an
// `equal`; the DAW uses ui/state/snapshot.ts); a commit with an unchanged snapshot is dropped. Each committed state gets an id; undo/redo return to the
// id they restore, so "unsaved" = current id ≠ id at save (no snapshot compare per edit: a snapshot of a large song
// costs 10–20 ms). The caller restores the returned snapshot (`restoreSong`) and refreshes its views.
interface Entry<S> { js: S; id: number }

export interface History<S = string> {
  /** Records `before` as an undo step when it differs from `after`; true if recorded. Clears redo. */
  commit(before: S, after: S): boolean;
  /** The snapshot to restore for undo (`now` goes to redo), or null when there is nothing to undo. */
  undo(now: S): S | null;
  redo(now: S): S | null;
  /** Forgets every step (a song was loaded); the current state gets a fresh id and a new epoch. */
  clear(): void;
  /** Generation, bumped by `clear`: a snapshot taken in an older epoch belongs to another song. */
  readonly epoch: number;
  readonly depth: number;
  readonly canRedo: boolean;
  /** Id of the current state; equal ids = same committed state. */
  readonly stateId: number;
}

export function createHistory<S = string>(equal: (a: S, b: S) => boolean = (a, b) => a === b): History<S> {
  const undoStack: Entry<S>[] = [], redoStack: Entry<S>[] = [];
  let cur = 0, next = 1, epoch = 0;
  const move = (from: Entry<S>[], to: Entry<S>[], now: S) => {
    const e = from.pop();
    if (!e) return null;
    to.push({ js: now, id: cur });
    cur = e.id;
    return e.js;
  };
  return {
    commit(before, after) {
      if (equal(before, after)) return false;
      undoStack.push({ js: before, id: cur });
      redoStack.length = 0;
      cur = next++;
      return true;
    },
    undo: (now) => move(undoStack, redoStack, now),
    redo: (now) => move(redoStack, undoStack, now),
    clear() { undoStack.length = redoStack.length = 0; cur = next++; epoch++; },
    get epoch() { return epoch; },
    get depth() { return undoStack.length; },
    get canRedo() { return redoStack.length > 0; },
    get stateId() { return cur; },
  };
}
