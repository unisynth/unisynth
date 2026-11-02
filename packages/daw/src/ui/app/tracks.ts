// Track commands of the running session (change daw-solid-ts 9.1, step 8): selection, add / rename / reorder /
// duplicate / delete (one undo step each, the views rebuild), the track header's events and menu, arm, volume (one undo
// step per gesture), mute / solo. Header and mixer strip call these (design D7).
import type { About, Device, UITrack } from "../../model.ts";
import { popMenu } from "../common/Menu.tsx";
import * as arrange from "../state/arrange.ts";
import { bumpLane } from "../state/arrange.ts";
import { raw } from "../state/song.ts";
import { setSelectedId } from "../state/view.ts";
import { devName } from "../model/params.ts";
import { fxOf, instOf } from "../model/track.ts";
import { armed, master, selected, setArmed, setSelected, tracks } from "./session.ts";
import { commit, say, snap } from "./core.ts";
import { livePush } from "./engine.ts";
import { rebuildTracks, setFocusArea } from "./arrange.ts";
import { arrKeys, clearClipSel, selClips } from "./clips.ts";
import { drawRoll, genRoll } from "./roll.ts";
import { renderAll, senders } from "./device.ts";

/** A new track's instrument: one oscillator through a low-pass filter. */
export const initInstrument = (): Device => ({ kind: 64, preset: "init", values: { "osc1.type": 0.16666667, "filt1.type": 0.1, "filt1.cutoff": 0.6, "env1.sustain": 0.7, "env1.release": 0.15 } });
/** Header device line: a bus's senders and effects, else the chain (`*` edited preset, "(off)" bypassed). */
export const devLabel = (t: UITrack): string => t.bus ? `Bus · ← ${senders(t).map(x => x.name).join(", ") || "no sends"}${fxOf(t).length ? " · " + fxOf(t).map(devName).join(" → ") : ""}`
  : t.devices.map(d => devName(d) + ((d as Device & { _edited?: boolean })._edited ? "*" : "") + (d.bypass ? " (off)" : "")).join(" → ") || "no devices";
/** One line from a song/2 about: type · style · tempo · engine · like … — description. */
export const aboutText = (a: About): string => [a.type, a.style, a.tempo, a.engine, a.similar_to?.length ? "like " + a.similar_to.join(", ") : ""].filter(Boolean).join(" · ") + " — " + a.description;
const focusHeader = (t: UITrack): void => { document.querySelector<HTMLElement>(`.thdr.track[data-track="${t.id}"]`)?.focus(); };
/** Binds the arrangement corner's "Add track" button. */
export function initTracks(): void {
  document.querySelector<HTMLElement>('#arr-corner button[aria-label="Add track"]')!.onclick = () => addTrack(false);
}

/** Selects `t` (the store proxy): arms an instrument track, opens its clip in the roll, shows its devices. */
export function selectTrack(t: UITrack): void {
  t = (t.master ? master : tracks.find(x => x.id === t.id)) || t;  // always the store proxy (writes must notify)
  setSelected(t); setSelectedId(t.id);
  if (!t.bus && !t.master && instOf(t) && armed !== t) { setArmed(t); updateArmed(); }  // the keyboard plays the selected track
  genRoll(t); renderAll();
  if (!document.getElementById("p-roll")!.hidden) drawRoll();
}
// Mute / solo (header and mixer strip buttons, design D7): one state per track, both views follow the store. Mute is
// one undo step (committed after the click); solo is not undoable.
export function toggleMuteSolo(t: UITrack, solo: boolean): void {
  const name = t.name, before = snap(), k = solo ? "solo" : "mute"; t[k] = !t[k];
  if (!solo) setTimeout(() => commit(before, `${name} ${t.mute ? "muted" : "unmuted"}`));
  say(`${name} ${solo ? (t.solo ? "soloed" : "unsoloed") : (t.mute ? "muted" : "unmuted")}`); livePush();
}
// ---- Tracks: add, rename, reorder, duplicate, delete ----
const TCOL = ["--t-kick", "--t-bass", "--t-lead", "--t-pad", "--t-keys", "--t-hats"];
const newId = (p1: string): string => p1 + Math.random().toString(36).slice(2, 7);
const firstBus = (): number => { const i = tracks.findIndex(x => x.bus); return i < 0 ? tracks.length : i; };
function trackCmd<T>(msg: string, fn: () => T): T { const before = snap(); const r = fn(); rebuildTracks(); selectTrack(selected); updateArmed(); commit(before, msg); return r; }
/** Adds an instrument track after the selected one (before the buses) or a bus at the end; selects it. */
export function addTrack(bus: boolean): void {
  const n = tracks.filter(x => !!x.bus === !!bus).length + 1;
  const t: UITrack = bus ? { id: newId("bus"), name: `Bus ${n}`, col: "--t-bus", vol: -6, bus: true, clips: [], pats: {}, devices: [{ kind: 71, values: { mode: 0, dry_wet: 1 } }], sends: [], lanes: [] }
    : { id: newId("t"), name: `Track ${n}`, col: TCOL[n % TCOL.length], vol: -6, clips: [], pats: {}, lanes: [], sends: [], devices: [initInstrument()] };
  const at = bus ? tracks.length : Math.min(firstBus(), tracks.indexOf(selected) + 1 || firstBus());
  trackCmd(`${t.name} added`, () => { tracks.splice(at, 0, t); setSelected(t); if (!bus) setArmed(t); });
  focusHeader(t);
}
function deleteTrack(t: UITrack): void {
  if (t.master) { say("the master cannot be deleted"); return; }
  if (!t.bus && tracks.filter(x => !x.bus).length === 1) { say("a song keeps at least one instrument track"); return; }
  const i = tracks.indexOf(t);
  trackCmd(`${t.name} deleted (Ctrl+Z restores it)`, () => {
    tracks.splice(i, 1); for (const x of tracks) x.sends = x.sends.filter(s1 => s1[0] !== t.name);
    if (selected === t) setSelected(tracks[Math.min(i, tracks.length - 1)]!); if (armed === t) setArmed(null);
  });
}
function moveTrack(t: UITrack, d: number): void {
  const i = tracks.indexOf(t), j = i + d; if (j < 0 || j >= tracks.length) return;
  trackCmd(`${t.name} moved ${d < 0 ? "up" : "down"}`, () => { tracks.splice(i, 1); tracks.splice(j, 0, t); });
  focusHeader(t);
}
function duplicateTrack(t: UITrack): void {
  const c: UITrack = JSON.parse(JSON.stringify(raw(t))); c.id = newId("t"); c.name = t.name + " 2"; c.clips.forEach(x => x.sel = false);
  trackCmd(`${t.name} duplicated`, () => { tracks.splice(tracks.indexOf(t) + 1, 0, c); setSelected(c); });
}
export function renameTrack(t: UITrack): void { arrange.setRenamingId(t.id); }
/** Rename: sends that named the track follow. */
export function renameTo(t: UITrack, v: string): void {
  const old = t.name;
  trackCmd(`${old} renamed to ${v || "(empty)"}`, () => { t.name = v; for (const x of [...tracks, master]) x.sends.forEach(s1 => { if (s1[0] === old) s1[0] = v; }); });
}
// Track header events (the header component calls these, design D7). Clicks on its buttons and fields are theirs.
export function headerClick(t: UITrack, e: MouseEvent): void {
  if ((e.target as Element).closest("button, input, select")) return;
  if (e.shiftKey || e.ctrlKey || e.metaKey) {  // Shift/Ctrl-click a header: add (or remove) all of its clips
    const all = t.clips.length && t.clips.every(c => c.sel); if (!e.shiftKey && !e.ctrlKey && !e.metaKey) clearClipSel();
    t.clips.forEach(c => c.sel = !all); tracks.forEach((x) => bumpLane(x.id)); say(`${selClips().length} clips selected`);
  }
  selectTrack(t); setFocusArea("arr");
}
// Keys on the focused header itself: F2 rename, Alt+↑/↓ reorder, Ctrl+Delete delete, menu key; then the clip
// commands (Ctrl/Delete/Escape), Enter/Space select, ↑/↓ move focus and selection between tracks.
export function headerKey(t: UITrack, e: KeyboardEvent, h: HTMLElement): void {
  if (e.target !== h) return;
  if (e.key === "F2") { e.preventDefault(); renameTrack(t); }
  else if (e.altKey && (e.key === "ArrowUp" || e.key === "ArrowDown")) { e.preventDefault(); e.stopPropagation(); moveTrack(t, e.key === "ArrowUp" ? -1 : 1); return; }
  else if ((e.ctrlKey || e.metaKey) && (e.key === "Delete" || e.key === "Backspace")) { e.preventDefault(); e.stopPropagation(); deleteTrack(t); return; }
  else if (e.key === "ContextMenu" || (e.shiftKey && e.key === "F10")) { e.preventDefault(); const r = h.getBoundingClientRect(); trackMenu(t, r.left + 40, r.top + 20); }
  const i = tracks.indexOf(t);
  if (e.ctrlKey || e.metaKey || e.key === "Delete" || e.key === "Backspace" || e.key === "Escape") { arrKeys(e, t); return; }
  if (e.key === "Enter" || e.key === " ") { e.preventDefault(); selectTrack(t); }
  if (e.key === "ArrowDown" || e.key === "ArrowUp") { e.preventDefault(); const n = tracks[Math.max(0, Math.min(tracks.length - 1, i + (e.key === "ArrowDown" ? 1 : -1)))]!; selectTrack(n); focusHeader(n); }
}
export function headerMenu(t: UITrack, e: MouseEvent): void { e.preventDefault(); selectTrack(t); trackMenu(t, e.clientX, e.clientY); }
function trackMenu(t: UITrack, x: number, y: number): void {
  const i = tracks.indexOf(t);
  popMenu(x, y, [["Rename", "F2 / double-click", () => renameTrack(t)], ["Duplicate track", "", () => duplicateTrack(t)],
    ["Move up", "Alt+↑", () => moveTrack(t, -1), i <= 0], ["Move down", "Alt+↓", () => moveTrack(t, 1), i >= tracks.length - 1], "-",
    ["Add instrument track", "", () => addTrack(false)], ["Add bus track", "", () => addTrack(true)], "-",
    ["Delete track", "Ctrl+Delete", () => deleteTrack(t)]], `${t.name} track menu`);
}
// ---- Arm, volume faders (header and mixer stay in sync) ----
export function updateArmed(): void {  // the footer (ui/footer/Audition.tsx) and the headers follow
  arrange.setArmedId(armed?.id ?? null);
}
export function toggleArm(t: UITrack): void {
  setArmed(armed === t ? null : t); updateArmed(); say(armed ? `${t.name} armed: the computer keyboard (Z–M, Q–P) and MIDI play it` : "no track armed");
}
// Volume (header and mixer faders, design D7): live while the slider moves, one undo step per gesture (`done` on
// change; a key press is a whole gesture). The other fader follows the store.
const volBefore = new Map<string, string>();
export function setVolume(t: UITrack, v: number, done?: boolean): void {
  if (done) { const b = volBefore.get(t.id); volBefore.delete(t.id); if (b) commit(b, `${t.name} volume ${v.toFixed(1)} dB`); return; }
  if (!volBefore.has(t.id)) volBefore.set(t.id, snap());
  t.vol = v; livePush();
}
