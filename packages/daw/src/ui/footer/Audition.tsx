// Footer audition controls (change daw-solid-ts 8.2): armed track with the keyboard octave and velocity, the ⌨
// toggle, the Web MIDI button and the MW/PB sliders (PB springs back). Values live in state/audition.ts; ui/app/audition.ts
// sends them to the engine (AuditionActions). Ids and labels are the pre-port ones. The sliders listen natively
// (`on:`), not by delegation: an `input`/`pointerup` dispatched without bubbling still counts.
import { type JSX } from "solid-js";
import { render } from "solid-js/web";
import * as au from "../state/audition.ts";

export interface AuditionActions {
  /** Name of the armed track, or null. */
  armedName(): string | null;
  toggleKb(): void;
  connectMidi(): void;
  wheel(v: number): void;
  bend(v: number): void;
}

export function mountAudition(slot: HTMLElement, a: AuditionActions): () => void {
  return render(() => <Audition a={a} />, slot);
}

function Audition(p: { a: AuditionActions }): JSX.Element {
  const a = p.a;
  const springBack = () => { if (au.pb() !== 0) a.bend(0); };
  return <>
    <span id="armed-info">Armed: <b>{a.armedName() ?? "none"}</b> · octave C{au.kbOct() - 1} · vel {au.kbVel()}</span>
    <button id="kb-btn" aria-pressed={au.kbOn()} onClick={() => a.toggleKb()}
      title="Computer keyboard plays the selected track like a tracker: Z–M and Q–P are notes, [ ] octave, ; ' velocity (`). Off: letters are shortcuts again">⌨</button>
    <button id="midi-info" title="Connect Web MIDI input to the armed track" onClick={() => a.connectMidi()}>{au.midiText()}</button>
    <span class="wheels" aria-label="Audition controllers">
      <label>MW <input id="mw" type="range" min="0" max="127" value={au.mw()} aria-label="Mod wheel (audition)"
        title="Mod wheel: the armed track's ctl.wheel (audition, not recorded)" on:input={(e) => a.wheel(+(e.currentTarget as HTMLInputElement).value)} /></label>
      <label>PB <input id="pb" type="range" min="-8192" max="8191" value={au.pb()} aria-label="Pitch bend (audition, springs back)"
        title="Pitch bend: the armed track's ctl.bend, ± its bend range (audition, not recorded; springs back)"
        on:input={(e) => a.bend(+(e.currentTarget as HTMLInputElement).value)} on:pointerup={springBack} on:keyup={springBack} /></label>
    </span>
  </>;
}
