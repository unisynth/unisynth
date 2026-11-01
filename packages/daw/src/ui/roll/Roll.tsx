// Piano roll panel (change daw-solid-ts 7.2, D4/D5): the note-editing toolbar, the audition keys, the note grid and the
// controller lane exist only while the Piano roll tab shows. Drawing and gestures stay imperative (D4): on mount the
// panel hands its elements to app.js (`attach`), which binds its handlers and paints the visible window of the
// canvases; on unmount `detach` drops them. Markup, ids and labels are the pre-port ones (tests, style.css).
import { Index, type JSX, onCleanup, onMount, Show } from "solid-js";
import { render } from "solid-js/web";
import { rollShown } from "../state/view.ts";

export interface RollEls {
  panel: HTMLElement; tools: HTMLElement; keys: HTMLElement; grid: HTMLElement; gridc: HTMLCanvasElement; rollPh: HTMLElement;
  vel: HTMLElement; velc: HTMLCanvasElement; ccKind: HTMLSelectElement; ccVal: HTMLElement; velSet: HTMLInputElement;
  qstr: HTMLSelectElement; rollInfo: HTMLElement; rollClip: HTMLElement;
}
export interface RollActions { attach(els: RollEls): void; detach(): void }

const NAMES = ["C", "C♯", "D", "D♯", "E", "F", "F♯", "G", "G♯", "A", "A♯", "B"];
/** Audition keys, top (HI) to bottom (LO): the order app.js indexes (`HI - note`). */
export const KEY_LO = 24, KEY_HI = 96;
const KEYS = Array.from({ length: KEY_HI - KEY_LO + 1 }, (_, k) => KEY_HI - k);

export function mountRoll(panel: HTMLElement, a: RollActions): () => void {
  return render(() => <Show when={rollShown()}><RollPanel panel={panel} a={a} /></Show>, panel);
}

function RollPanel(p: { panel: HTMLElement; a: RollActions }): JSX.Element {
  const el = {} as RollEls;
  el.panel = p.panel;
  onMount(() => p.a.attach(el));
  onCleanup(() => p.a.detach());
  const zb = (z: string, label: string, title: string, text: string) => <button data-zoom={z} aria-label={label} title={title}>{text}</button>;
  return <>
    <div id="roll-tools" role="toolbar" aria-label="Note editing" ref={(x) => (el.tools = x)}>
      <button data-roll="cut" title="Cut (Ctrl+X)">Cut</button><button data-roll="copy" title="Copy (Ctrl+C)">Copy</button><button data-roll="paste" title="Paste at the cursor (Ctrl+V)">Paste</button><button data-roll="dup" title="Duplicate after the selection (Ctrl+D)">Duplicate</button><button data-roll="del" title="Delete (Del)">Delete</button>
      <span class="sep" />
      <span class="dim">Transpose</span><button data-roll="t-12" title="Octave down (Shift+↓)">−12</button><button data-roll="t-1" title="Semitone down (↓)">−1</button><button data-roll="t+1" title="Semitone up (↑)">+1</button><button data-roll="t+12" title="Octave up (Shift+↑)">+12</button>
      <span class="sep" />
      <button data-roll="quant" title="Quantize starts to the snap (Q)">Quantize</button><button data-roll="quantlen" title="Quantize starts and lengths (Shift+Q)">+ lengths</button>
      <label class="dim">Strength <select id="qstr" aria-label="Quantize strength" ref={(x) => (el.qstr = x)}><option value="1">100%</option><option value=".75">75%</option><option value=".5">50%</option></select></label>
      <button data-roll="legato" title="Extend each note to the next one">Legato</button>
      <span class="sep" />
      <label class="dim">Vel <input id="vel-set" type="number" min="1" max="127" step="1" disabled aria-label="Velocity of the selected notes" title="Set the velocity of every selected note" ref={(x) => (el.velSet = x)} /></label>
      <span class="sep" />
      <button data-roll="all" title="Select all (Ctrl+A)">Select all</button>
      <span class="sep" />
      <span class="dim">Zoom</span>{zb("t-", "Zoom out in time", "Time zoom out (−; Ctrl/⌘+wheel or pinch)", "−")}{zb("t+", "Zoom in in time", "Time zoom in (+)", "+")}{zb("p-", "Smaller key rows", "Pitch zoom out (Alt+wheel)", "↕−")}{zb("p+", "Taller key rows", "Pitch zoom in (Alt+wheel)", "↕+")}{zb("fit", "Fit clip", "Fit the clip and its notes (Shift+Z)", "⤢")}
      <span id="roll-info" class="dim num" aria-live="polite" ref={(x) => (el.rollInfo = x)} />
      <span id="roll-clip" class="num" style={{ "margin-left": "auto" }} ref={(x) => (el.rollClip = x)} />
    </div>
    <div id="roll">
      <div id="keys" aria-label="Keyboard, click to audition" ref={(x) => (el.keys = x)}>
        <Index each={KEYS}>{(n) => { const nm = NAMES[n() % 12]!, oct = Math.floor(n() / 12) - 1;
          return <button class={nm.includes("♯") ? "blk" : ""} aria-label={`Audition ${nm}${oct}`}>{nm === "C" ? `C${oct}` : ""}</button>; }}</Index>
      </div>
      <div id="grid" ref={(x) => (el.grid = x)}><div id="roll-ph" class="playhead" hidden ref={(x) => (el.rollPh = x)} />
        <canvas id="gridc" tabindex="0" ref={(x) => (el.gridc = x)} aria-label="Notes. Click selects, drag moves, edge resizes, double-click adds or deletes, drag on empty space selects a range. Arrows move and transpose." /></div>
      <div id="vel-lbl"><select id="cc-kind" aria-label="Controller lane" ref={(x) => (el.ccKind = x)}><option value="vel">Velocity</option><option value="wheel">Mod wheel</option><option value="bend">Pitch bend</option></select><span id="cc-val" class="num" aria-live="polite" ref={(x) => (el.ccVal = x)} /></div>
      <div id="vel" ref={(x) => (el.vel = x)}><canvas id="velc" tabindex="0" ref={(x) => (el.velc = x)} aria-label="Controller lane: velocity. Drag a stem to set that note; on a selected note it sets the selection (Ctrl or Cmd: that note only, Alt: all equal). Drag from an empty spot to paint. Mouse wheel nudges." /></div>
    </div>
  </>;
}
