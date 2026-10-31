// Mixer UI state (change daw-solid-ts): whether the panel shows, and the edit actions app.js provides. The strips
// read the song store (state/song.ts) and the meters (state/live.ts) directly. Kept apart from the components
// (mixer.tsx) so a hot reload of a component keeps the state: only edited modules re-run.
import { createSignal } from "solid-js";
import type { Device } from "../model.ts";

/** Edits, implemented in app.js on the song store (each one commits its own undo step). */
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
  /** Display name of a device (engine table label / preset). */
  devName(d: Device): string;
}

export const [shown, setShown] = createSignal(false);
export function setMixerShown(on: boolean): void { setShown(on); }
