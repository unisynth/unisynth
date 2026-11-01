// Sampler block of an oscillator slot (change daw-solid-ts 6.3): source (GSM sample of the instrument / GM.DLS
// instrument / GM.DLS wave), what plays, the GSM buttons (play, choose, import, remove unused), and for a chosen sample
// its playback settings plus the waveform with start and loop (D4: drawn on a canvas by an effect; drag the start
// line, a loop edge or the loop body). `osc<i>.s7` = source (choice of 3), `osc<i>.sample` = raw index, shape values
// s0–s6 (crates/unisynth/src/smp.rs). Markup and labels are the pre-port ones (contract tests).
import { createEffect, createMemo, For, type JSX, onCleanup, Show } from "solid-js";
import type { Device, Param, Track } from "./Device.tsx";
import { registerLive } from "./live.ts";

export interface SamplerActions {
  ip(name: string): Param;
  vget(d: Device, p: Param): number;
  /** The value shown now (automation at the playhead, else stored); a store read. */
  curVal(d: Device, name: string): number;
  choice(v: number, n: number): number;
  setParam(t: Track, d: Device, p: Param, v: number, msg: string): void;
  begin(): unknown;
  /** What the slot plays (one line). */
  smpWhat(d: Device, i: number): string;
  /** No GM bank (or `?nobank`). */
  gmMissing(): boolean;
  /** Number of GSM samples no slot plays (0: no Remove button). */
  gsmUnused(d: Device): number;
  playSample(b64: string): void;
  openLibrary(src: string): void;
  importWav(t: Track, d: Device, slot: number): void;
  removeUnusedGsm(t: Track, d: Device): void;
  /** Decoded GSM sample (44.1 kHz mono), cached. */
  pcm(b64: string): Float32Array | null;
  /** A drag step: value + engine, no undo step. */
  put(t: Track, d: Device, p: Param, v: number): void;
  /** End of a waveform drag: marks the preset edited, one undo step. */
  dragEnd(t: Track, d: Device, before: unknown, msg: string): void;
}

const SOURCES = ["GSM sample", "GM instrument (per-note regions)", "GM wave"];
const LOOP_MODES = ["Off", "Repeat", "Ping-pong"], LOOP_REGIONS = ["Sample's own loop", "Custom"], INTERPS = ["Nearest", "Linear"];
const SETTING = ["", "loop", "loop region", "", "", "reverse", "interpolation"];
const W = 360, H = 52;
const tokens = new Map<string, string>();  // static theme colours (no theme switching), as app.js's css()
const css = (v: string) => { let c = tokens.get(v); if (c === undefined) tokens.set(v, (c = getComputedStyle(document.documentElement).getPropertyValue(v).trim())); return c; };

export function Sampler(p: { t: Track; d: Device; i: number; a: SamplerActions }): JSX.Element {
  const { t, d, i, a } = p, S = (k: number | string) => `osc${i}.s${k}`;
  const src = () => a.choice(a.vget(d, a.ip(S(7))), 3), idx = () => Math.round(a.vget(d, a.ip(`osc${i}.sample`)));
  const nGsm = () => d.samples?.length ?? 0;
  const b64 = () => src() === 0 && idx() >= 1 && idx() <= nGsm() ? d.samples![idx() - 1]! : null;
  const settings = () => src() === 0 ? b64() !== null : idx() >= 1;
  return <div class="sampler">
    <div class="srow"><label class="pchoice"><span>source</span>
      <select aria-label={`Osc ${i} sample source`} onChange={(e) => { const k = +e.currentTarget.value; a.setParam(t, d, a.ip(S(7)), k / 2, `${t.name}: osc ${i} source ${SOURCES[k]}`); }}>
        <For each={SOURCES}>{(x, k) => <option value={k()} selected={k() === src()}>{x}</option>}</For>
      </select></label>
      <span class="dim">{a.smpWhat(d, i)}</span>
      <Show when={src() === 0} fallback={<button class="gmpick" onClick={() => a.openLibrary("gm")}>Choose GM instrument…</button>}>
        <Show when={b64()}>{(b) => <button class="gsmplay" aria-label={`Play GSM sample ${idx()}`} onClick={() => a.playSample(b())}>▶ Play</button>}</Show>
        <button class="gsmpick" onClick={() => a.openLibrary("gsm")}>Choose GSM sample…</button>
        <button class="gsmimport" onClick={() => a.importWav(t, d, i)}>Import WAV…</button>
        <Show when={a.gsmUnused(d)}>{(n) => <button class="gsmclean" title="Samples no slot plays still cost bytes in the intro" onClick={() => a.removeUnusedGsm(t, d)}>Remove unused ({n()})</button>}</Show>
      </Show>
    </div>
    <Show when={src() !== 0}>
      <Show when={!a.gmMissing()} fallback={<div class="warn-note" role="alert">GM bank not found: set GM_DLS or install the OS bank. The song keeps the index and plays once a bank is present.</div>}>
        <div class="info-note">Uses the OS bank at run time (Windows gm.dls, macOS gs_instruments.dls): sounds differ per platform.{src() === 2 ? " One wave plays over all keys; the GM instrument source picks a region per note." : ""}</div>
      </Show>
    </Show>
    <Show when={settings()}><Settings t={t} d={d} i={i} a={a} b64={b64} /></Show>
  </div>;
}

function Settings(p: { t: Track; d: Device; i: number; a: SamplerActions; b64: () => string | null }): JSX.Element {
  const { t, d, i, a } = p, S = (k: number) => `osc${i}.s${k}`, cv = (k: number) => a.curVal(d, S(k));
  const opt = (k: number, n: number) => a.choice(a.vget(d, a.ip(S(k))), n);
  const set = (k: number, j: number, text: string) => { const n = k === 1 ? 3 : 2; a.setParam(t, d, a.ip(S(k)), j / (n - 1), `${t.name}: osc ${i} ${SETTING[k]} ${text}`); };
  const sel = (label: string, k: number, opts: string[]) =>
    <label class="pchoice"><span>{label}</span><select data-s={k} aria-label={`Osc ${i} ${label}`} onChange={(e) => set(k, +e.currentTarget.value, e.currentTarget.selectedOptions[0]!.text)}>
      <For each={opts}>{(o, j) => <option value={j()} selected={j() === opt(k, opts.length)}>{o}</option>}</For></select></label>;
  // the waveform: peaks per pixel column, drawn normalised
  const wave = createMemo(() => {
    const b = p.b64(), pcm = b ? a.pcm(b) : null, peaks = new Float32Array(W * 2);
    if (pcm?.length) for (let x = 0; x < W; x++) {
      let lo = 0, hi = 0; const s0 = Math.floor((x * pcm.length) / W), s1 = Math.max(s0 + 1, Math.floor(((x + 1) * pcm.length) / W));
      for (let k = s0; k < s1; k++) { lo = Math.min(lo, pcm[k]!); hi = Math.max(hi, pcm[k]!); }
      peaks[2 * x] = lo; peaks[2 * x + 1] = hi;
    }
    let pk = 0; for (const x of peaks) pk = Math.max(pk, Math.abs(x));
    return { pcm, peaks, gain: pk > 1e-4 ? 0.95 / pk : 1 };
  });
  const loopOf = (): [number, number] | null => {
    const on = a.choice(cv(1), 3), custom = a.choice(cv(2), 2); if (!on) return null;
    if (!custom) return wave().pcm ? [0, 1] : null;
    const s0 = cv(3); return [s0, Math.min(1, s0 + cv(4))];
  };
  const label = () => {
    const { pcm } = wave(), lp = loopOf(), rev = a.choice(cv(5), 2), st = cv(0), secs = pcm ? pcm.length / 44100 : 0;
    return `Osc ${i} sample${secs ? ` ${secs.toFixed(2)} s` : ""}: start ${(st * 100).toFixed(1)} %${rev ? " (reverse)" : ""}${lp ? `, loop ${(lp[0] * 100).toFixed(1)}–${(lp[1] * 100).toFixed(1)} %` : ", no loop"}`;
  };
  let cvs!: HTMLCanvasElement;
  createEffect(() => {
    const { pcm, peaks, gain } = wave(), lp = loopOf(), rev = a.choice(cv(5), 2), st = cv(0), sx = (rev ? 1 - st : st) * W;
    const g = cvs.getContext("2d")!, dpr = devicePixelRatio;
    g.setTransform(dpr, 0, 0, dpr, 0, 0); g.clearRect(0, 0, W, H);
    g.fillStyle = css("--bg"); g.fillRect(0, 0, W, H);
    if (lp) { g.fillStyle = css("--focus") + "33"; g.fillRect(lp[0] * W, 0, (lp[1] - lp[0]) * W, H); g.fillStyle = css("--focus"); g.fillRect(lp[0] * W, 0, 2, H); g.fillRect(lp[1] * W - 2, 0, 2, H); }
    g.fillStyle = css("--fg") + "aa";
    if (pcm?.length) for (let x = 0; x < W; x++) { const y0 = H / 2 - peaks[2 * x + 1]! * gain * H / 2, y1 = H / 2 - peaks[2 * x]! * gain * H / 2; g.fillRect(x, y0, 1, Math.max(1, y1 - y0)); }
    else { g.fillStyle = css("--dim"); g.font = "10px sans-serif"; g.fillText("GM bank sample: waveform not shown; positions are fractions of it", 6, H / 2 + 3); }
    const ok = css("--ok") || "#4c4";
    g.strokeStyle = ok; g.lineWidth = 2; g.beginPath(); g.moveTo(sx, 0); g.lineTo(sx, H); g.stroke();
    g.fillStyle = ok; g.beginPath(); g.moveTo(sx, 0); g.lineTo(sx + (rev ? -7 : 7), 0); g.lineTo(sx, 7); g.fill();
  });
  const put = (k: number, v: number) => a.put(t, d, a.ip(S(k)), Math.min(1, Math.max(0, v)));
  const down = (e: PointerEvent) => {
    const r = cvs.getBoundingClientRect(), f = (x: number) => (x - r.left) / r.width, f0 = f(e.clientX), near = (q: number, px = 5) => Math.abs((f0 - q) * r.width) <= px;
    const lp = loopOf(), rev = a.choice(cv(5), 2), st = cv(0), sPos = rev ? 1 - st : st, custom = a.choice(cv(2), 2);
    const what = near(sPos) ? "start" : lp && near(lp[0]) ? "ls" : lp && near(lp[1]) ? "le" : lp && custom && f0 > lp[0] && f0 < lp[1] ? "move" : "start";
    const before = a.begin();
    if (!custom && what !== "start" && lp) { put(2, 1); put(3, lp[0]); put(4, lp[1] - lp[0]); }  // dragging the sample's own loop makes it custom
    const l0 = cv(3), len0 = cv(4); cvs.setPointerCapture(e.pointerId);
    const mv = (ev: PointerEvent) => {
      const x = Math.min(1, Math.max(0, f(ev.clientX)));
      if (what === "start") put(0, rev ? 1 - x : x);
      else if (what === "ls") { const end = l0 + len0; put(3, Math.min(x, end - 0.001)); put(4, end - cv(3)); }
      else if (what === "le") put(4, Math.max(0.001, x - l0));
      else put(3, Math.min(1 - len0, Math.max(0, l0 + x - f0)));
    };
    mv(e); cvs.addEventListener("pointermove", mv);
    cvs.addEventListener("pointerup", () => {
      cvs.removeEventListener("pointermove", mv);
      a.dragEnd(t, d, before, `${t.name}: osc ${i} sample ${{ start: "start", ls: "loop start", le: "loop end", move: "loop" }[what]} moved`);
    }, { once: true });
  };
  return <>
    <div class="srow smpset">
      {sel("loop", 1, LOOP_MODES)}
      <Show when={opt(1, 3)}>{sel("region", 2, LOOP_REGIONS)}</Show>
      {sel("interp", 6, INTERPS)}
      <label class="pchoice"><input type="checkbox" data-s="5" checked={!!opt(5, 2)} aria-label={`Osc ${i} reverse`}
        onChange={(e) => set(5, +e.currentTarget.checked, e.currentTarget.checked ? "on" : "off")} /><span>reverse</span></label>
    </div>
    <div class="smpwrap">
      <canvas ref={cvs} class="smpwave" width={W * devicePixelRatio} height={H * devicePixelRatio} style={{ width: `${W}px`, height: `${H}px` }}
        role="img" aria-label={label()} title={`${label()} · drag the start line or the loop`} onPointerDown={down} />
      <svg class="smplive" viewBox="0 0 100 40" preserveAspectRatio="none" aria-hidden="true" data-live={`smp${i}`} ref={(el) => onCleanup(registerLive(el, `smp${i}`))} />
    </div>
  </>;
}
