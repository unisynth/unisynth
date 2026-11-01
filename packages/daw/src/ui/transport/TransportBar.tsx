// Transport bar in SolidJS (change daw-solid-ts 3.3): the page header. Same ids, classes, names and titles as the old
// static markup (tests and style.css rely on them). Reads state/transport.ts and state/live.ts; edits go through
// `TransportActions` (app.js). Position and time derive from the playhead, tempo and signature, so a tempo change
// updates the time at once. Save shows the unsaved state (ui/state/files.ts); Bounce / Export / Save call actions.
import { createEffect, createMemo, For, type JSX } from "solid-js";
import { render } from "solid-js/web";
import { load, playing, pos } from "../state/live.ts";
import * as tr from "../state/transport.ts";
import { dirty, saveLabel, songFile } from "../state/files.ts";

export interface TransportActions {
  play(): void;
  stop(): void;
  toggleRec(): void;
  toggleLoop(): void;
  toggleFollow(): void;
  zoomOut(): void;
  zoomIn(): void;
  zoomFit(): void;
  tempo(bpm: number): void;
  sig(s: string): void;
  snap(s: string): void;
  /** "start – end" typed into the loop field; false when it is not a valid range (the field is restored). */
  loopText(s: string): boolean;
  length(bars: number): void;
  buffer(v: string): void;
  openFilters(on: boolean): void;
  undo(): void;
  redo(): void;
  save(): void;
  bounce(): void;
  exportSong(): void;
}

export function mountTransport(header: HTMLElement, a: TransportActions): () => void {
  header.replaceChildren();
  return render(() => <TransportBar a={a} />, header);
}

const fmtBars = (b: number) => String(+b.toFixed(2));

function TransportBar(p: { a: TransportActions }): JSX.Element {
  const a = p.a;
  const posText = createMemo(() => tr.fmtPos(pos(), tr.sig()));
  const timeText = createMemo(() => tr.fmtTime(pos(), tr.tempo(), tr.sig()));
  const loopText = () => `${fmtBars(tr.loop()[0])} – ${fmtBars(tr.loop()[1])}`;
  let loopIn!: HTMLInputElement, lenIn!: HTMLInputElement, tempoIn!: HTMLInputElement;
  // the loop field follows the loop unless the user is typing in it
  createEffect(() => { const v = loopText(); if (document.activeElement !== loopIn) loopIn.value = v; });
  const field = (label: string, input: JSX.Element) => <label class="field">{label}{input}</label>;
  return <>
    <div class="group">
      <button id="play" aria-label={playing() ? "Pause (Space)" : "Play (Space)"} title="Play (Space)" onClick={() => a.play()}>{playing() ? "❚❚" : "▶"}</button>
      <button id="stop" aria-label="Stop" title="Stop; press again to return to the start" onClick={() => a.stop()}>■</button>
      <button id="rec" aria-pressed={tr.rec()} aria-label="Record automation" title="Record automation (Shift+R): while playing, a knob you turn or hold writes its automation lane (the lane is created if needed)" onClick={() => a.toggleRec()}>●</button>
      <button id="loop-btn" aria-pressed={tr.loopOn()} aria-label="Loop" title="Loop (L); Ctrl+L loops the selected clips" onClick={() => a.toggleLoop()}>⟲</button>
      <button id="follow" aria-pressed={tr.follow()} classList={{ held: tr.followHeld() }} aria-label="Follow playhead" onClick={() => a.toggleFollow()}
        title={tr.followHeld() ? "Follow paused because you scrolled · resumes when the playhead reaches the right edge, or click / F" : "Follow playhead (F): the view pages along when the playhead reaches the right edge"}>⇥ Follow</button>
      <button id="zout" aria-label="Zoom out arrangement" title="Zoom out (−; Ctrl/⌘+wheel or pinch over the lanes; wheel over the ruler)" onClick={() => a.zoomOut()}>−</button><button id="zin" aria-label="Zoom in arrangement" title="Zoom in (+)" onClick={() => a.zoomIn()}>+</button><button id="zfit" aria-label="Zoom to fit" title="Fit the selected clips, or the whole song (Shift+Z)" onClick={() => a.zoomFit()}>⤢</button>
    </div>
    <div class="sep" />
    <div class="group">
      <span class="pos big num" aria-label="Position bars beats ticks" id="pos">{posText()}</span>
      <span class="pos num" aria-label="Position time" id="time">{timeText()}</span>
    </div>
    <div class="sep" />
    {field("Tempo", <input ref={tempoIn} class="num" type="number" min="20" max="400" step="0.5" aria-label="Tempo in BPM" value={tr.tempo()}
      onChange={(e) => { const v = +e.currentTarget.value; if (v > 0) a.tempo(v); else tempoIn.value = String(tr.tempo()); }} />)}
    {field("Signature", <select id="sig" title="Display only: the beat grid and bars.beats.ticks follow it; one tempo, no meter changes" value={tr.sig()} onChange={(e) => a.sig(e.currentTarget.value)}>
      <For each={tr.sigOptions()}>{(o) => <option selected={o === tr.sig()}>{o}</option>}</For>
    </select>)}
    {field("Snap", <select id="snap" value={tr.snap()} onChange={(e) => a.snap(e.currentTarget.value)}>
      <For each={tr.SNAPS}>{(o) => <option selected={o === tr.snap()}>{o}</option>}</For>
    </select>)}
    {field("Loop", <input ref={loopIn} id="loop-in" class="num" aria-label="Loop range in bars, start – end" title="Type e.g. 5 – 13, or drag the blue bar in the ruler" style={{ width: "72px" }}
      onChange={(e) => { const el = e.currentTarget; if (a.loopText(el.value)) el.blur(); el.value = loopText(); }} />)}
    {field("Length", <input ref={lenIn} id="len-in" class="num" type="number" min="1" aria-label="Song length in bars" title="Grows by itself when clips go past the end" style={{ width: "52px" }} value={tr.bars()}
      onChange={(e) => { a.length(Math.max(1, Math.round(+e.currentTarget.value || 0))); lenIn.value = String(tr.bars()); }} />)}
    <div class="sep" />
    <div class="group" title="Audio engine: render time per 128-frame call vs real time">
      <span class="dim">Engine</span><span class="load" role="meter" aria-valuenow={load()} aria-valuemin="0" aria-valuemax="100" aria-label="Engine load"><i style={{ width: `${load()}%` }} /></span><span class="num dim">{load()}%</span>
      <span id="drops" class="drops" classList={{ bad: (tr.drops()?.n ?? 0) > 0, dim: (tr.drops()?.n ?? 0) === 0 }} hidden={!tr.drops()} title={tr.drops()?.title ?? "Audio drops: moments the sound card got no samples in time and played silence"}>
        {`${tr.drops()?.n ?? 0} drop${tr.drops()?.n === 1 ? "" : "s"}`}
      </span>
      <label class="dim" title="Output buffer. Larger rides out moments when the computer is busy (no clicks or dropouts); smaller makes keyboard audition respond sooner.">Buffer <select id="buffer" aria-label="Audio output buffer" value={tr.buffer()} onChange={(e) => a.buffer(e.currentTarget.value)}>
        <For each={tr.BUFFERS}>{([v, label]) => <option value={v} selected={v === tr.buffer()}>{label}</option>}</For>
      </select></label>
      <label class="dim" title="Listening switch for comparing with WaveSabre renders: run filters that are fully open instead of skipping them. The original engines run them, which takes about 4 dB off at 16 kHz. Live playback only: the song, Bounce and Export are unchanged.">
        <input type="checkbox" id="openflt" checked={tr.openFilters()} onChange={(e) => a.openFilters(e.currentTarget.checked)} /> Run open filters</label>
    </div>
    <span id="edited" class="badge warn" hidden={!tr.edited()} title="An edit happened since play started; live sound may differ from the intro. Bounce for the exact render.">edited since play</span>
    <span class="spacer" />
    <div class="group">
      <button title="Undo (Ctrl+Z)" aria-label="Undo" onClick={() => a.undo()}>↶</button>
      <button title="Redo (Ctrl+Shift+Z)" aria-label="Redo" onClick={() => a.redo()}>↷</button>
      <button id="bounce-btn" title="Render the song or the loop to WAV" onClick={() => a.bounce()}>Bounce…</button>
      <button id="export-btn" title="Export the song blob next to the song" onClick={() => a.exportSong()}>Export</button>
      <button id="save" classList={{ dirty: dirty().on }} aria-label={saveLabel(songFile(), dirty())} title="Save (Ctrl+S)" onClick={() => a.save()}>Save</button>
    </div>
  </>;
}
