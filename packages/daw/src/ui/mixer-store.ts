// Mixer state (change daw-solid-ts): the view app.js pushes and the meter levels. Kept apart from the components
// (mixer.tsx) so a hot reload of a component keeps the state: only edited modules re-run.
import { createSignal } from "solid-js";
import { createStore, reconcile } from "solid-js/store";

export interface InsertView { id: string; i: number; name: string; bypass: boolean }
export interface SendView { id: string; i: number; to: string; side: boolean; db: number }
export interface StripView {
  id: string; name: string; col: string; master: boolean; bus: boolean; sel: boolean;
  vol: number; mute: boolean; solo: boolean; inserts: InsertView[]; sends: SendView[];
}
export interface MixerView { tracks: StripView[]; master: StripView | null }

/** Edits, implemented in app.js on the song model (each one commits its own undo step). */
export interface MixerActions {
  select(id: string): void;
  openInsert(id: string): void;
  bypass(id: string, i: number): void;
  effects(): string[];
  addInsert(id: string, label: string): void;
  sendTargets(id: string): [string, "main" | "side"][];
  addSend(id: string, to: string, ch: "main" | "side"): void;
  removeSend(id: string, i: number): void;
  /** Live while the slider moves; `done` once at the end of the gesture (one undo step). */
  sendLevel(id: string, i: number, db: number, done: boolean): void;
  addBus(): void;
}

export const [view, setView] = createStore<MixerView>({ tracks: [], master: null });
export const [peaks, setPeaks] = createStore<Record<string, [number, number]>>({});
export const [shown, setShown] = createSignal(false);

/** Replaces the mixer's view; only the strips/fields that differ re-render. */
export function syncMixer(v: MixerView): void { setView(reconcile(v, { key: "id" })); }
/** One strip's fields (fader, mute/solo): a path update, no view rebuild or diff of the other strips. */
export function patchStrip(id: string, p: Partial<StripView>): void {
  if (view.master?.id === id) setView("master", p as StripView); else setView("tracks", (t) => t.id === id, p);
}
/** One send level of a strip (while its slider moves). */
export function patchSend(id: string, i: number, db: number): void {
  if (view.master?.id === id) return;
  setView("tracks", (t) => t.id === id, "sends", (sd) => sd.i === i, "db", db);
}
/** Meter level of one track (0..1, L/R); callers skip invisible changes. */
export function setPeak(id: string, lr: [number, number]): void { setPeaks(id, lr); }
export function setMixerShown(on: boolean): void { setShown(on); }

