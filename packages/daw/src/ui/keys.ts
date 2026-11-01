// Global keyboard shortcuts (change daw-solid-ts 8.1, design D7): every key the page handles outside a focused
// editor, in one place, with the precedence the docs table promises (docs/synth/daw.md):
//   1. before the focused editor (window, capture): Ctrl/⌘+S from anywhere; ` toggles the computer keyboard; while
//      it plays (armed track, no modifier, not typing, no menu) its note, octave and velocity keys, which then reach
//      no editor and no letter shortcut;
//   2. the focused editor (arrangement, piano roll, headers, menus: their own keydown handlers);
//   3. after it (document, bubble): undo/redo unless the editor took the key; then, outside text fields and
//      sliders, transport letters and the zoom keys of the area with focus.
// Two listeners because one phase cannot run both before and after the editor.

export interface KeyActions {
  save(): void;
  undo(): void;
  redo(): void;
  /** Computer keyboard: toggled by `; plays while `kbPlays` (armed track, keyboard on). */
  toggleKb(): void;
  kbPlays(): boolean;
  kbNoteOn(i: number, code: string): void;
  kbNoteOff(code: string): void;
  octave(d: 1 | -1): void;
  velocity(d: 1 | -1): void;
  allNotesOff(): void;
  playPause(): void;
  toHome(): void;
  toggleLoop(): void;
  loopSelection(): void;
  toggleFollow(): void;
  toggleRec(): void;
  /** The piano roll has focus (for zoom keys); otherwise the arrangement zooms. */
  inRoll(e: KeyboardEvent): boolean;
  zoomRoll(f: number): void;
  zoomArr(f: number): void;
  fitRoll(): void;
  fitArr(): void;
}

/** Tracker layout by physical key: Z row = one octave from C, Q row = the next (Z = C, Q = C above). */
export const KB_LO = ["KeyZ", "KeyS", "KeyX", "KeyD", "KeyC", "KeyV", "KeyG", "KeyB", "KeyH", "KeyN", "KeyJ", "KeyM"];
export const KB_HI = ["KeyQ", "Digit2", "KeyW", "Digit3", "KeyE", "KeyR", "Digit5", "KeyT", "Digit6", "KeyY", "Digit7", "KeyU", "KeyI", "Digit9", "KeyO", "Digit0", "KeyP"];
/** Semitone of a key code above the keyboard octave's C, or -1. */
export const kbNote = (code: string): number => { const i = KB_LO.indexOf(code); if (i >= 0) return i; const j = KB_HI.indexOf(code); return j >= 0 ? 12 + j : -1; };
const OCT_DN = ["BracketLeft", "PageDown", "NumpadDivide"], OCT_UP = ["BracketRight", "PageUp", "NumpadMultiply"];
const VEL_DN = ["Semicolon", "NumpadSubtract"], VEL_UP = ["Quote", "NumpadAdd"];

const el = (e: Event) => e.target as HTMLElement;
const typing = (e: Event) => { const t = el(e); return t instanceof HTMLInputElement || t instanceof HTMLSelectElement || t instanceof HTMLTextAreaElement || t.isContentEditable; };
const mod = (e: KeyboardEvent) => e.ctrlKey || e.metaKey;

export function installKeys(a: KeyActions): () => void {
  const early = (e: KeyboardEvent) => {
    if (mod(e) && e.key.toLowerCase() === "s") { e.preventDefault(); e.stopImmediatePropagation(); a.save(); return; }
    if (e.code === "Backquote" && !mod(e) && !typing(e)) { e.preventDefault(); a.toggleKb(); return; }
    if (!a.kbPlays() || mod(e) || e.altKey || e.shiftKey || typing(e) || el(e).closest?.(".menu")) return;
    const stop = () => { e.preventDefault(); e.stopImmediatePropagation(); };
    const i = kbNote(e.code);
    if (i >= 0) { stop(); if (!e.repeat) a.kbNoteOn(i, e.code); }
    else if (OCT_DN.includes(e.code) || OCT_UP.includes(e.code)) { stop(); a.octave(OCT_UP.includes(e.code) ? 1 : -1); }
    else if (VEL_DN.includes(e.code) || VEL_UP.includes(e.code)) { stop(); a.velocity(VEL_UP.includes(e.code) ? 1 : -1); }
  };
  const earlyUp = (e: KeyboardEvent) => { if (kbNote(e.code) >= 0) a.kbNoteOff(e.code); };
  const late = (e: KeyboardEvent) => {
    const t = el(e);
    // undo/redo anywhere outside text fields (the arrangement and piano roll handle theirs and prevent the default)
    if (mod(e) && !e.defaultPrevented && !(t instanceof HTMLInputElement && t.type !== "checkbox") && !(t instanceof HTMLTextAreaElement)) {
      const k = e.key.toLowerCase();
      if (k === "z") { e.preventDefault(); return e.shiftKey ? a.redo() : a.undo(); }
      if (k === "y") { e.preventDefault(); return a.redo(); }
    }
    if (t instanceof HTMLInputElement || t instanceof HTMLSelectElement) return;
    const k = e.key.toLowerCase();
    if (t.getAttribute?.("role") !== "slider") {  // a knob keeps Space, Home and letters
      if (e.code === "Space") { e.preventDefault(); a.playPause(); return; }
      if (k === "l" && mod(e)) { e.preventDefault(); a.loopSelection(); return; }
      if (k === "l" && !e.altKey) { a.toggleLoop(); return; }
      if (k === "f" && !mod(e)) { a.toggleFollow(); return; }
      if (e.key === "R" && e.shiftKey && !mod(e)) { a.toggleRec(); return; }
      if (e.key === "Home") { a.toHome(); return; }
    }
    // + / − zoom (time) and Shift+Z fits, in the area that has focus (a focused knob too)
    if (mod(e) || e.altKey) return;
    const f = e.key === "+" || e.key === "=" ? 1.5 : e.key === "-" || e.key === "_" ? 1 / 1.5 : 0;
    if (f) { e.preventDefault(); a.inRoll(e) ? a.zoomRoll(f) : a.zoomArr(f); }
    else if (e.key === "Z" && e.shiftKey) { e.preventDefault(); a.inRoll(e) ? a.fitRoll() : a.fitArr(); }
  };
  const blur = () => a.allNotesOff();
  window.addEventListener("keydown", early, true);
  window.addEventListener("keyup", earlyUp, true);
  window.addEventListener("blur", blur);
  document.addEventListener("keydown", late);
  return () => {
    window.removeEventListener("keydown", early, true); window.removeEventListener("keyup", earlyUp, true);
    window.removeEventListener("blur", blur); document.removeEventListener("keydown", late);
  };
}
