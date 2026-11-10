// Device panel (change daw-solid-ts 6.2): the selected track's instrument and effect chain as components over the
// song store. Knobs, selects and slot state follow the store, so an edit or an automation step updates only what
// reads it; nothing rebuilds the panel except a track change. Drawing stays imperative (design D4): each slot screen
// is the element ui/app/screens.ts builds, redrawn by an effect from the values its `_draw` reads; the sampler block is
// ui/device/Sampler.tsx. Markup, classes and labels are the pre-port ones (contract tests).
import { createEffect, createMemo, createSignal, For, Index, type JSX, on, onCleanup, Show, untrack } from "solid-js";
import { render } from "solid-js/web";
import { Knob } from "../common/Knob.tsx";
import { Sampler, type SamplerActions } from "./Sampler.tsx";
import { chips, markLiveDirty, registerLive } from "./live.ts";
import { autoOverridden, autoValue, blockClosed, deviceShown, setBlockClosed, setSlotOpen, shownTrack, slotOpenOf } from "../state/device.ts";
import { GAP, SLOTS } from "../model/params.ts";

export interface Param { name: string; default: number; law: string; options: string[]; raw?: boolean; structural?: boolean; index: number }
export interface Table { kind: number; mode: number; label: string; params: Param[] }
export interface Device { kind: number; preset?: string; values: Record<string, number>; bypass?: boolean; _edited?: boolean; samples?: string[] }
export interface Track { id: string; name: string; master?: boolean; bus?: boolean; devices: Device[]; lanes: { dev: number; param: string }[] }
type G = "osc" | "filt" | "env" | "lfo";

export interface DeviceActions extends SamplerActions {
  ip(name: string): Param;
  tableOf(d: Device): Table | undefined;
  nModes(kind: number): number;
  devName(d: Device): string;
  fmtVal(d: Device, p: Param, v: number): string;
  /** Stored value or the parameter default (store read). */
  vget(d: Device, p: Param): number;
  choice(v: number, n: number): number;
  oscType(d: Device, i: number): number;
  smpShapes(d: Device, i: number): number[];
  slotUsed(d: Device, g: G | "mod", i: number): boolean;
  routesOf(d: Device, g: "env" | "lfo", i: number): string[];
  modSrcName(d: Device, m: number): string;
  senders(t: Track): string[];
  /** Tracks that send to `t`'s side input (a compressor's sidechain). */
  sideSenders(t: Track): string[];
  /** A select / number edit: one undo step. */
  setParam(t: Track, d: Device, p: Param, v: number, msg: string): void;
  /** A knob step: model, engine, record or override, preset `*`. */
  knobInput(t: Track, d: Device, p: Param, v: number): void;
  grab(t: Track, d: Device, p: Param): void;
  release(t: Track, d: Device, p: Param): void;
  begin(): unknown;
  end(before: unknown, msg: string): void;
  /** An effect's mode changed: keep only the mode (another parameter set). */
  modeChanged(t: Track, d: Device): void;
  clearSlot(t: Track, d: Device, g: G, i: number): void;
  swapFilters(t: Track, d: Device, i: number): void;
  bypass(t: Track, i: number): void;
  move(t: Track, i: number, dir: -1 | 1): void;
  remove(t: Track, i: number): void;
  effects(): string[];
  addEffect(t: Track, label: string): void;
  browse(): void;
  importSample(t: Track, d: Device): void;
  savePreset(t: Track, d: Device, btn: HTMLElement): void;
  liveOn(): boolean;
  toggleLive(): void;
  /** Slot screen element with `_draw` (osc/filter/LFO/envelope), or null. */
  screen(t: Track, d: Device, g: G, i: number): (Element & { _draw?: () => void }) | null;
  /** The panel's status line (aria-live), shown under the mod matrix. */
  status: HTMLElement;
}

export { SLOTS };
const SLOT_NOUN = { osc: "Osc", filt: "Filter", env: "Env", lfo: "LFO" };
const OSC_COMMON = ["tracking", "coarse", "fine", "octave", "ratio", "ratio_fine", "fixed_hz", "gain", "mix", "pan", "pm_out", "feedback"];
const OSC_SHAPES: Record<number, (string | null)[]> = { 1: ["waveform"], 2: ["waveform", "pulse width"], 3: ["bandwidth", "harmonics", "damp", "scale", "mod", "drive", "layers", "layer detune", "layer spread"],
  4: ["start", null, null, "loop start", "loop length"],
  // the naive generators (5–11): the colour; type, combine and law are their own parameters
  5: [null, "colour"], 6: [null, "colour"], 7: [null, "colour"], 8: [null, "colour"], 9: [null, "colour"], 10: [null, "colour"], 11: [null, "colour"],
  12: ["wave", "voices", "version", "clock", "retrigger", "filter base", "filter amount", "filter decay", "filter damp"],
  // Points (add-tf3-import): point k's offset and value at s(2k), s(2k+1), drive at s8; count, segment modes and
  // layers are their own parameters (POINT_FIELDS)
  13: ["p1 offset", "p1 value", "p2 offset", "p2 value", "p3 offset", "p3 value", "p4 offset", "p4 value", "drive"],
  // PAD table: s0–s4 build the table when the instrument loads (a change rebuilds the device), drive plays live
  14: ["bandwidth", "scale", "damp", "harmonics", "profile", "drive"],
  // Pair (add-elevated-import): Elevated's two naive copies at ratios 2 − d and d; s1 the phase shift, s2 = d/2
  15: ["wave", "phase shift", "detune", "polarity"] };  // s1 loop mode, s2 boundary, s5 reverse, s6 interp, s7 source: samplerPre; index: osc<i>.sample
const range = (n: number) => [...Array(n)].map((_, k) => k + 1);
/** A Points slot's own fields and their labels (`osc<i>.<field>`); `mode<k>` ends at point k. */
const POINT_LABELS: Record<string, string> = { points: "points", mode1: "seg 1", mode2: "seg 2", mode3: "seg 3", mode4: "seg 4", mode_last: "last seg",
  layers: "layers", layer_detune: "layer detune", layer_spread: "layer spread" };
/** Points shown for a count value (`Count(1, 4)`, unisynth `law::count`). */
const pointCount = (v: number): number => 1 + Math.min(3, Math.floor(Math.max(0, Math.min(1, v)) * 3 + .5));

let A!: DeviceActions;
/** Display name; tracks the mode and preset (params.ts `devName` reads the kind and mode raw). */
const devName = (d: Device) => { void d.values.mode; void d.preset; return A.devName(d); };

/** Mounts the panel into `el`: the shown track's chain while the Device tab shows, nothing otherwise. */
export function mountDevice(el: HTMLElement, actions: DeviceActions): () => void {
  A = actions;
  return render(() => <Show when={deviceShown() && shownTrack() as Track | null} keyed>{(t) => <Chain t={t} />}</Show>, el);
}

function Chain(p: { t: Track }): JSX.Element {
  const t = p.t, inst = createMemo(() => t.devices.find((d) => d.kind === 64));
  const [picking, setPicking] = createSignal(false);
  return <div class="chain">
    <Show when={inst()} keyed fallback={<div class="noinst"><b>{t.master ? "Master" : "Bus"}</b><span class="dim">{t.master ? "end of the graph → song output" : "no instrument · fed by sends from " + (A.senders(t).join(", ") || "nothing")}</span></div>}>
      {(d) => <Instrument t={t} d={d} />}
    </Show>
    <For each={t.devices}>{(d, i) => <Show when={d.kind !== 64}><FxBox t={t} d={d} i={i} /></Show>}</For>
    <Show when={picking()} fallback={<button class="addfx" onClick={() => setPicking(true)}>+ Effect</button>}>
      <select aria-label={`Effect to add to ${t.name}`} ref={(s) => queueMicrotask(() => s.focus())}
        onChange={(e) => { const v = e.currentTarget.value; if (v) { setPicking(false); A.addEffect(t, v); } }}
        onBlur={(e) => { if (!e.currentTarget.value) setPicking(false); }}>
        <option value="">choose effect…</option>
        <For each={A.effects()}>{(x) => <option>{x}</option>}</For>
      </select>
    </Show>
  </div>;
}

const LAZY_OPTIONS = 16;
/** One parameter control: a select (choices), a number field (raw), else a knob. */
function PCtl(p: { t: Track; d: Device; p: Param; label?: string; aria?: string }): JSX.Element {
  const { t, d } = p, prm = p.p, short = p.label ?? prm.name.slice(prm.name.indexOf(".") + 1).replace(/_/g, " ");
  const aria = () => `${devName(d)} ${prm.name}`;
  if (prm.law === "Index" || prm.options.length) {
    const idx = prm.law === "Index";
    const cur = () => idx ? Math.round(A.vget(d, prm) * 255) : A.choice(A.vget(d, prm), prm.options.length);
    // long lists (mod targets: 142) hold the current option only until the select is first used: a selection
    // remounts the panel, and these options were two thirds of its elements (daw-solid-ts 9.4)
    const shown = prm.options.filter((o) => o !== GAP).length; // free positions of an expanded choice are not offered
    const [full, setFull] = createSignal(shown <= LAZY_OPTIONS), fill = () => setFull(true);
    return <label class="pchoice"><span>{short}</span>
      <select aria-label={aria()} onPointerDown={fill} onFocus={fill} onKeyDown={fill} onChange={(e) => {
        const i = +e.currentTarget.value, v = idx ? i / 255 : prm.options.length > 1 ? i / (prm.options.length - 1) : 0;
        A.setParam(t, d, prm, v, `${t.name}: ${prm.name} = ${prm.options[i]}`);
        if (prm.name === "mode" && d.kind !== 64) A.modeChanged(t, d);
      }}>
        <Show when={full()} fallback={<option value={cur()} selected>{prm.options[cur()]}</option>}>
          <Index each={prm.options}>{(o, i) => <Show when={o() !== GAP || i === cur()}><option value={i} selected={i === cur()}>{o()}</option></Show>}</Index></Show>
      </select></label>;
  }
  if (prm.raw) {
    return <label class="pchoice"><span>{short}</span>
      <input type="number" class="num" min="0" max="65535" step="1" value={Math.round(A.vget(d, prm))} aria-label={aria()}
        onChange={(e) => { const n = e.currentTarget; A.setParam(t, d, prm, Math.max(0, Math.min(65535, Math.round(+n.value || 0))), `${t.name}: ${prm.name} = ${n.value}`); }} /></label>;
  }
  // automated: the lane's value at the playhead unless overridden by hand (applyAutomation keeps it current)
  const lane = () => { const di = t.devices.indexOf(d); return t.lanes.some((l) => l.dev === di && l.param === prm.name); };
  const ovr = () => lane() && autoOverridden(d.values, prm.name);
  const value = () => autoValue(d.values, prm.name) ?? A.vget(d, prm);
  return <Knob label={short} aria={p.aria} value={value} text={(v) => A.fmtVal(d, prm, v)} def={prm.default} param={prm.name}
    auto={lane} ovr={ovr}
    title={() => ovr() ? "Automation overridden by hand · Re-enable automation to follow the lane again" : lane() ? "Automated: follows its lane" : prm.name}
    input={(v) => A.knobInput(t, d, prm, v)} begin={A.begin} end={A.end}
    onGrab={() => A.grab(t, d, prm)} onRelease={() => A.release(t, d, prm)} />;
}

interface GroupOpts { cols: number; labels?: Record<string, string>; pre?: () => (Element & { _draw?: () => void }) | null; post?: () => JSX.Element; slot?: { g: G; i: number } }
/** A group panel: title, choice controls in its header row, knobs below; optional screen (`pre`) and slot chrome. */
function PGroup(p: { t: Track; d: Device; title: string; ps: () => Param[]; o: GroupOpts }): JSX.Element {
  const { t, d } = p, o = p.o, isChoice = (x: Param) => x.law === "Index" || x.options.length > 0 || !!x.raw;
  const s = o.slot, key = s && `${t.id}:${s.g}${s.i}`, used = () => !!s && A.slotUsed(d, s.g, s.i);
  const closed = () => !!s && !(slotOpenOf(key!) ?? used());
  const pre = createMemo(() => closed() ? null : untrack(() => o.pre?.() ?? null));  // built only while open
  // the screen follows the values its drawing reads (D4: drawing stays imperative)
  createEffect(() => { const el = pre(); if (el?._draw) { el._draw(); markLiveDirty(); } });  // a redraw drops the overlay
  // screens carry per-voice overlays (ui/device/live.ts) while mounted
  createEffect(() => { const el = pre(); if (el && s && el.matches("svg.scr, svg.env")) { el.setAttribute("data-live", `${s.g}${s.i}`); onCleanup(registerLive(el, `${s.g}${s.i}`)); } });
  const title = s ? `${SLOT_NOUN[s.g]} ${s.i}` : p.title;
  const routes = () => s && (s.g === "env" || s.g === "lfo") ? (s.g === "env" && s.i === 1 ? ["amplitude"] : A.routesOf(d, s.g, s.i)) : null;
  const knobs = () => p.ps().filter((x) => x && !isChoice(x)), choices = () => p.ps().filter((x) => x && isChoice(x));
  return <div class="grp" classList={{ slot: !!s, off: !!s && !used(), closed: closed(), unrouted: !!routes() && !routes()!.length }} role="group" aria-label={p.title}>
    <div class="gh">
      <Show when={s}><button class="tw" aria-expanded={!closed()} aria-label={`${closed() ? "Expand" : "Collapse"} ${title}`} onClick={() => setSlotOpen(key!, closed())}>{closed() ? "▸" : "▾"}</button></Show>
      <span>{p.title}</span>
      <For each={choices()}>{(x) => <PCtl t={t} d={d} p={x} label={o.labels?.[x.name]} />}</For>
      <Show when={s}>
        <span class="sp" />
        <Show when={s!.g === "env" && s!.i === 1} fallback={<Show when={used()}>
          <button class="clr" title={`Reset ${title} (later slots stay where they are)`} aria-label={`Clear ${title}`} onClick={() => { setSlotOpen(key!, undefined); A.clearSlot(t, d, s!.g, s!.i); }}>✕</button></Show>}>
          <span class="dim" title="Envelope 1 shapes the amplitude and ends the voice">fixed</span>
        </Show>
      </Show>
    </div>
    <Show when={routes()}>{(r) => <div class="rt">{r().length ? `→ ${r().join(", ")}` : "unrouted · knobs inactive"}</div>}</Show>
    {/* a collapsed slot renders only its header (D5) */}
    <Show when={!closed()}>
      {pre()}
      {o.post?.()}
      <Show when={knobs().length}>
        <div class="knobs" style={{ "--cols": o.cols }}>
          <For each={knobs()}>{(x) => <PCtl t={t} d={d} p={x} label={o.labels?.[x.name]} />}</For>
        </div>
      </Show>
    </Show>
  </div>;
}

/**
 * A Points slot's points (add-tf3-import): a column per point (offset: the fraction of the span left after the previous
 * point; value; the mode of the segment that ends at it) and the last segment's mode, back to the cycle's end.
 */
function PointsGrid(p: { t: Track; d: Device; i: number; n: () => number }): JSX.Element {
  const { t, d, i } = p;
  return <div class="pts" role="group" aria-label={`Osc ${i} points`}>
    <Index each={range(4)}>{(k) => <Show when={k() <= p.n()}>
      <div class="pt"><span class="pth">P{k()}</span>
        <PCtl t={t} d={d} p={A.ip(`osc${i}.s${2 * k() - 2}`)} label="offset" />
        <PCtl t={t} d={d} p={A.ip(`osc${i}.s${2 * k() - 1}`)} label="value" />
        <PCtl t={t} d={d} p={A.ip(`osc${i}.mode${k()}`)} label="to here" /></div>
    </Show>}</Index>
    <div class="pt"><span class="pth">end</span><PCtl t={t} d={d} p={A.ip(`osc${i}.mode_last`)} label="to end" /></div>
  </div>;
}

function Block(p: { k: string; label: string; count: () => string; children: JSX.Element }): JSX.Element {
  const closed = () => blockClosed(p.k);
  return <div class="blk" classList={{ closed: closed() }} data-k={p.k} role="group" aria-label={p.label}>
    <div class="blk-h"><button class="tw" aria-expanded={!closed()} aria-label={`${closed() ? "Expand" : "Collapse"} ${p.label}`} onClick={() => setBlockClosed(p.k, !closed())}>{closed() ? "▸" : "▾"}</button>
      <span>{p.label}</span><span class="cnt num">{p.count()}</span></div>
    <Show when={!closed()}><div class="blk-list">{p.children}</div></Show>
  </div>;
}

function Instrument(p: { t: Track; d: Device }): JSX.Element {
  const { t, d } = p, P = (names: string[]) => names.map((n) => A.ip(n));
  const used = (g: G | "mod") => `${range(SLOTS[g]).filter((i) => A.slotUsed(d, g, i)).length}/${SLOTS[g]}`;
  const [live, setLive] = createSignal(A.liveOn());
  const osc = (i: number) => {
    const ty = createMemo(() => A.oscType(d, i));
    // the controls a slot shows follow its kind, the sampler's source and loop, and a Points slot's point count
    const shape = createMemo(() => ty() === 4 ? A.smpShapes(d, i).join() : ty() === 13 ? String(pointCount(A.vget(d, A.ip(`osc${i}.points`)))) : "");
    const labels = () => {
      const sh = OSC_SHAPES[ty()] || [], l: Record<string, string> = Object.fromEntries(sh.map((nm, k) => [`osc${i}.s${k}`, nm!]));
      if (ty() === 4) l[`osc${i}.sample`] = "sample #";
      if (ty() === 13) for (const [f, nm] of Object.entries(POINT_LABELS)) l[`osc${i}.${f}`] = nm;
      return l;
    };
    const ps = createMemo(on([ty, shape], () => {
      const sh = OSC_SHAPES[ty()] || [];
      // a Points slot's offsets, values and segment modes are its Points grid (`post`); count, drive and layers knobs
      const ks = ty() === 4 ? A.smpShapes(d, i) : ty() === 13 ? [8] : sh.map((_, k) => k);
      const own = ty() === 4 ? [`osc${i}.sample`] : ty() === 13 ? ["points", "layers", "layer_detune", "layer_spread"].map((f) => `osc${i}.${f}`) : [];
      return [A.ip(`osc${i}.type`), ...OSC_COMMON.map((f) => A.ip(`osc${i}.${f}`)), ...ks.map((k) => A.ip(`osc${i}.s${k}`)), ...own.map((nm) => A.ip(nm))];
    }));
    // the screen: a waveform drawing, or the sampler block (6.3)
    const pre = () => { const k = ty(); return k === 4 ? <Sampler t={t} d={d} i={i} a={A} /> as HTMLElement : k ? A.screen(t, d, "osc", i) : null; };
    const post = () => ty() === 13 ? <PointsGrid t={t} d={d} i={i} n={() => +shape()} /> : null;
    return <Show when={ty() + 1} keyed>{(_k) => <PGroup t={t} d={d} title={`Osc ${i}`} ps={ps} o={{ cols: 6, get labels() { return labels(); }, pre, post, slot: { g: "osc", i } }} />}</Show>;
  };
  const card = (g: "filt" | "env" | "lfo", i: number, names: string[], cols: number) => {
    return <PGroup t={t} d={d} title={`${SLOT_NOUN[g]} ${i}`} ps={() => P(names)} o={{ cols, pre: () => A.screen(t, d, g, i), slot: { g, i } }} />;
  };
  return <div class="devbox" classList={{ byp: !!d.bypass }} aria-label="Instrument">
    <h3>Unisynth <span class="kind">kind 64 · preset {d.preset || "init"}{d._edited ? "*" : ""}</span><span class="vox" role="group" aria-label="Sounding voices">
      <Index each={chips().list}>{(c) => <i style={{ "--vc": c().col }} classList={{ rel: c().rel }}>{c().label}</i>}</Index>
      <Show when={chips().more}><b>+{chips().more}</b></Show></span>
      <span class="acts"><button class="livev" aria-pressed={live()} title="Live view: show the sounding voices on the screens (costs some engine time while on)" onClick={() => { A.toggleLive(); setLive(A.liveOn()); }}>Live</button>
        <button class="browse" onClick={() => A.browse()}>Browse…</button>
        <button class="impsmp" title="Encode an audio file (WAV, AIFF, MP3…) as a GSM sample and play it on a sampler slot (a free oscillator if there is no GSM sampler)" onClick={() => A.importSample(t, d)}>Import sample…</button>
        <button class="savepre" onClick={(e) => A.savePreset(t, d, e.currentTarget)}>Save preset</button>
        <button class="bypass" aria-pressed={!!d.bypass} title="Bypass the instrument" aria-label="Bypass instrument" onClick={() => A.bypass(t, t.devices.indexOf(d))}>⏻</button></span></h3>
    <div class="devbody">
      <div class="dcol c-voice" data-k="voice">
        <PGroup t={t} d={d} title="Voice" ps={() => P(["mode", "steal", "one_shot", "env1_at_output", "master", "pan", "capacity", "unison", "uni_detune", "uni_spread", "slide", "glide", "transpose", "velocity", "start_spread", "noise_level", "drop.base", "drop.rate"])} o={{ cols: 4 }} />
        <PGroup t={t} d={d} title="Noise" ps={() => P(["noise.band_on", "noise.hp", "noise.lp"])} o={{ cols: 2 }} />
      </div>
      <div class="dcol"><Block k="osc" label="Oscillators" count={() => used("osc")}>{range(3).map(osc)}</Block></div>
      <div class="dcol"><Block k="filt" label="Filters · serial" count={() => used("filt")}>
        {range(SLOTS.filt).map((i) => <>
          <Show when={i > 1}><button class="swap" title={`Swap filter ${i - 1} and ${i} (serial order; mod targets and lanes follow)`} aria-label={`Swap Filter ${i - 1} and Filter ${i}`} onClick={() => {
            const a = slotOpenOf(`${t.id}:filt${i - 1}`), b = slotOpenOf(`${t.id}:filt${i}`);
            A.swapFilters(t, d, i); setSlotOpen(`${t.id}:filt${i - 1}`, b); setSlotOpen(`${t.id}:filt${i}`, a);
          }}>⇅</button></Show>
          {card("filt", i, [`filt${i}.type`, `filt${i}.cutoff`, `filt${i}.resonance`], 2)}
        </>)}
      </Block></div>
      <div class="dcol"><Block k="env" label="Envelopes" count={() => used("env")}>
        {range(4).map((i) => card("env", i, [`env${i}.law`, `env${i}.attack`, `env${i}.decay`, `env${i}.sustain`, `env${i}.release`, `env${i}.curve_slope`, `env${i}.linear`, `env${i}.hold`], 3))}
      </Block></div>
      <div class="dcol">
        <Block k="lfo" label="LFOs" count={() => used("lfo")}>{range(2).map((i) => card("lfo", i, [`lfo${i}.shape`, `lfo${i}.sync`, `lfo${i}.rate`, `lfo${i}.depth`], 2))}</Block>
        <Block k="mod" label="Mod matrix" count={() => used("mod")}><ModTable t={t} d={d} /></Block>
        {A.status}
      </div>
    </div>
  </div>;
}

/** Mod matrix: one row per slot (source, target, operation, amount); rows with source None dimmed. */
function ModTable(p: { t: Track; d: Device }): JSX.Element {
  const { t, d } = p;
  return <table class="mm"><thead><tr><th /><th>source</th><th>target</th><th>op</th><th>amount</th></tr></thead><tbody>
    {range(SLOTS.mod).map((m) => {
      const src = () => A.modSrcName(d, m);
      return <tr class={src() === "None" ? "off" : ""} data-src={src() === "None" ? "" : /^Env/.test(src()) ? "env" : /^LFO/.test(src()) ? "lfo" : "voice"}>
        <td class="n num">{m}</td>
        {["source", "target", "operation"].map((f) => <td><PCtl t={t} d={d} p={A.ip(`mod${m}.${f}`)} label="" /></td>)}
        <td><PCtl t={t} d={d} p={A.ip(`mod${m}.amount`)} label="" aria={`mod${m} amount`} /></td>
      </tr>;
    })}
  </tbody></table>;
}

function FxBox(p: { t: Track; d: Device; i: () => number }): JSX.Element {
  const { t, d } = p;
  const tb = createMemo(() => { void d.values.mode; return A.tableOf(d); });  // the mode picks the parameter set
  const name = () => devName(d);
  const fx = () => t.devices.map((x, k) => [x, k] as const).filter(([x]) => x.kind !== 64).map((e) => e[1]), pos = () => fx().indexOf(p.i());
  return <div class="devbox" classList={{ byp: !!d.bypass }} aria-label={`Effect ${name()}`}>
    <h3>{tb() ? tb()!.label.split(" · ")[0] : name()} <span class="kind">kind {d.kind}{d.kind === 72 && A.sideSenders(t).length ? " · sidechain ← " + A.sideSenders(t).join(", ") : ""}</span>
      <span class="acts"><button class="mvl" title="Move earlier in the chain" aria-label={`Move ${name()} earlier`} disabled={pos() <= 0} onClick={() => A.move(t, p.i(), -1)}>◀</button>
        <button class="mvr" title="Move later in the chain" aria-label={`Move ${name()} later`} disabled={pos() === fx().length - 1} onClick={() => A.move(t, p.i(), 1)}>▶</button>
        <button class="bypass" aria-pressed={!!d.bypass} title="Bypass" aria-label={`Bypass ${name()}`} onClick={() => A.bypass(t, p.i())}>⏻</button>
        <button title="Remove" aria-label={`Remove ${name()}`} onClick={() => A.remove(t, p.i())}>✕</button></span></h3>
    <div class="devbody">
      <Show when={tb()} keyed fallback={<div class="warn-note">kind {d.kind}: no parameter table</div>}>
        {(table) => { const ps = table.params.filter((x) => x.name !== "mode" || A.nModes(d.kind) > 1);
          return <PGroup t={t} d={d} title={table.label} ps={() => ps} o={{ cols: Math.min(3, ps.length) }} />; }}
      </Show>
    </div>
  </div>;
}
