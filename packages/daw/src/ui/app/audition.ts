// Audition of the running session (change daw-solid-ts 9.1, step 9; never recorded): notes on a track's instrument
// from the computer keyboard, the piano-roll keys, library entries and Web MIDI, plus the mod wheel and pitch bend.
//
// Computer keyboard as a tracker's note keyboard (FastTracker 2 / Renoise layout, by physical key so QWERTZ and other
// layouts play the same): Z S X D C V G B H N J M = one octave from C, Q 2 W 3 E R 5 T 6 Y 7 U I 9 O 0 P = the next
// octave and a half. It plays the selected track (arming follows selection). Octave [ ] (PageDown/PageUp, numpad / *),
// velocity ; ' (numpad − +), ` or the ⌨ button toggles; Shift/Ctrl/Alt combinations stay shortcuts. The keys are read
// by ui/keys.ts; this holds the actions.
import type { UITrack } from "../../model.ts";
import type { LibItem, Source } from "../state/library.ts";
import * as au from "../state/audition.ts";
import { paramOf } from "../model/params.ts";
import { engineDev, instOf } from "../model/track.ts";
import { armed, E, selected } from "./session.ts";
import { say } from "./core.ts";
import { ensureAudio } from "./engine.ts";
import { trackIndex } from "./device.ts";
import { keyLight } from "./roll.ts";
import { gsmOf, playSample } from "./samples.ts";

/** Sounding notes by the key that holds them ("k<code>", "m<note>", "mouse", "lib" …). */
const heldV = new Map<string, { ti: number; note: number }>();
// Mod wheel and pitch bend (sliders, Web MIDI CC1 / pitch bend) set the armed track's instrument controller block
// (unisynth ctl.wheel / ctl.bend) live; never written to the song. A seek or stop restores the stored values.
function ctlLive(name: string, v: number): void {
  const t = armed, d = t && instOf(t), p = d && paramOf(d, name), di = t && d ? engineDev(t, d) : -1;
  if (t && p && di >= 0 && E.node) E.setParam(trackIndex(t), di, p.index, v);
}
export function setWheel(v: number): void { au.setMw(v); ctlLive("ctl.wheel", v / 127); }
export function setBend(v: number): void { au.setPb(v); ctlLive("ctl.bend", (v + 8192) / 16383); }
/** Plays `note` on `t`'s instrument, held as `key` (a key already holding a note releases it first). */
export function noteOn(t: UITrack | null, note: number, vel: number, key: string): void {
  if (!t || t.bus || t.master || note < 0 || note > 127 || !instOf(t)) return; noteOff(key);
  const ti = trackIndex(t);
  heldV.set(key, { ti, note }); keyLight(note, true);
  ensureAudio().then(ok => { if (ok && heldV.get(key)?.note === note) E.noteOn(ti, note, vel); });
}
export function noteOff(key: string): void { const h = heldV.get(key); if (!h) return; if (E.node) E.noteOff(h.ti, h.note); heldV.delete(key); if (![...heldV.values()].some(x => x.note === h.note)) keyLight(h.note, false); }
export function allNotesOff(): void { [...heldV.keys()].forEach(noteOff); }
/** Library "Audition": a GSM entry plays its sample, others C3 (a GM kit C1) on the selected track's instrument. */
export function auditionEntry(src: Source, it: LibItem, done: () => void): void {
  if (src.id === "gsm") { gsmOf(it.gsm).then(b64 => { if (b64) playSample(b64); }); done(); return; }
  const n = src.id === "gm" && it.gm?.drums ? 36 : 48;
  noteOn(selected, n, au.kbVel(), "lib"); setTimeout(() => { noteOff("lib"); done(); }, 900);
}
export function toggleKb(): void { const on = !au.kbOn(); au.setKbOn(on); say(on ? "computer keyboard plays the selected track (Z–M, Q–P)" : "computer keyboard off: letters are shortcuts again"); }
/** Web MIDI (asks the browser on first use): notes to the armed track, CC1 to MW, pitch bend to PB. */
export function connectMidi(): void {
  if (!navigator.requestMIDIAccess) { au.setMidiText("MIDI: not available in this browser"); return; }
  navigator.requestMIDIAccess().then(m => {
    const hook = (): void => { let n = 0; m.inputs.forEach(i => { i.onmidimessage = midiMsg; n++; }); au.setMidiText(n ? `MIDI: ${n} input${n > 1 ? "s" : ""} → ${armed ? armed.name : "arm a track"}` : "MIDI: no input connected"); };
    hook(); m.onstatechange = hook;
  }).catch(() => { au.setMidiText("MIDI: permission denied"); });
}
function midiMsg(e: MIDIMessageEvent): void {
  const [st = 0, d1 = 0, d2 = 0] = e.data ?? [], cmd = st & 0xf0;
  if (cmd === 0x90 && d2) noteOn(armed, d1, d2, "m" + d1);
  else if (cmd === 0x80 || (cmd === 0x90 && !d2)) noteOff("m" + d1);
  else if (cmd === 0xb0 && d1 === 1) setWheel(d2);
  else if (cmd === 0xe0) setBend(((d2 << 7) | d1) - 8192);
}
