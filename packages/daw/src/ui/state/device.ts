// Device panel view state (change daw-solid-ts 6.2): never saved, never undoable.
// - auto values: the value of each automated parameter at the playhead, per device `values` object. Written by
//   ui/app/automation.ts's applyAutomation (only on change; it runs every frame while playing), read by the knobs, so playback
//   moves just the automated knobs. Whether a lane exists comes from the song store; the override flag from here.
// - slot and block collapse (block collapse persists in the browser, state/persist.ts).
import { batch, createSignal } from "solid-js";
import { createStore, type SetStoreFunction } from "solid-js/store";
import { persist } from "./persist.ts";
import { raw } from "./song.ts";

type Auto = { v: Record<string, number | undefined>; ovr: Record<string, boolean | undefined> };
const autos = new WeakMap<object, [Auto, SetStoreFunction<Auto>]>();
function autoOf(values: object): [Auto, SetStoreFunction<Auto>] {
  const r = raw(values);
  let a = autos.get(r);
  if (!a) autos.set(r, (a = createStore<Auto>({ v: {}, ovr: {} })));
  return a;
}
/** Value at the playhead (undefined: not computed / overridden) and override flag of `values[field]`. */
export function setAuto(values: object, field: string, v: number | undefined, ovr: boolean): void {
  const [a, set] = autoOf(values);
  if (a.v[field] === v && !!a.ovr[field] === ovr) return;
  batch(() => { set("v", field, v); set("ovr", field, ovr); });
}
export const autoValue = (values: object, field: string): number | undefined => autoOf(values)[0].v[field];
export const autoOverridden = (values: object, field: string): boolean => !!autoOf(values)[0].ovr[field];

/** Slot collapse for the session, per track + slot (`<track id>:<g><i>`); unset = open when the slot is used. */
const [slotOpen, setSlotOpenStore] = createStore<Record<string, boolean | undefined>>({});
export const slotOpenOf = (key: string): boolean | undefined => slotOpen[key];
export const setSlotOpen = (key: string, v: boolean | undefined): void => { setSlotOpenStore(key, v); };

const [blk, setBlk] = createStore<Record<string, boolean>>({});
/** Block collapse (Oscillators, Filters, …), remembered in the browser (`daw.blk.<k>`), never in the song. */
export function blockClosed(k: string): boolean {
  const v = blk[k];  // tracked even while unset
  return v ?? persist.get(`daw.blk.${k}`) === "1";
}
export function setBlockClosed(k: string, closed: boolean): void { persist.set(`daw.blk.${k}`, closed ? "1" : "0"); setBlk(k, closed); }

/** The track the panel shows: renderDevice() publishes the session's `selected` (assigned in several places). */
export const [shownTrack, setShownTrackSig] = createSignal<unknown>(null, { equals: false });
export const setShownTrack = (t: unknown): void => { if (t !== shownTrack()) setShownTrackSig(() => t); };

/** The Device tab shows: the panel is mounted only then (design D5; spec "hidden panels hold no content"). */
export const [deviceShown, setDeviceShown] = createSignal(false);
