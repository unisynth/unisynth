// Audition state (change daw-solid-ts 8.2): the computer keyboard (on/off, octave, velocity), the Web MIDI line
// and the MW/PB sliders. Live only: never in the song, never undoable, not remembered.
import { createSignal } from "solid-js";

export const [kbOn, setKbOn] = createSignal(true);
/** Octave of the Z key's C (4 = MIDI 48, shown as C3) and note velocity 1–127. */
export const [kbOct, setKbOctRaw] = createSignal(4);
export const [kbVel, setKbVelRaw] = createSignal(100);
export const setKbOct = (v: number): void => { setKbOctRaw(Math.max(0, Math.min(8, v))); };
export const setKbVel = (v: number): void => { setKbVelRaw(Math.max(1, Math.min(127, v))); };
/** Web MIDI button text ("MIDI: connect" until the browser grants access). */
export const [midiText, setMidiText] = createSignal("MIDI: connect");
/** Mod wheel 0–127 and pitch bend −8192…8191 (springs back to 0 on release). */
export const [mw, setMw] = createSignal(0);
export const [pb, setPb] = createSignal(0);
