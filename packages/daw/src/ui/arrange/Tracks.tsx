// Track headers and lanes (change daw-solid-ts 5.2, design D6): one row per track in two columns (#hdrs, #lanes),
// windowed by ui/state/arrange.ts (rows in view ± one screen, the selected and focused row always), spacers keep
// offsets and the scroll extent. Same classes, ids, data-track and ARIA as the old rows. Headers are reactive on the
// song store (name, mute/solo, volume, lanes, open) and the view state (selected, armed, renaming). Clips and
// automation lanes are still filled imperatively when a row mounts (`mountLane`, `mountAuto`; refilled on `rev`);
// they move to components in 5.3. The volume, mute/solo and arm clicks are still document delegates in app.js (5.4).
import { createEffect, createMemo, For, mapArray, on, onCleanup, onMount, Show, untrack, type JSX } from "solid-js";
import { insert, render } from "solid-js/web";
import { selectedId } from "../state/view.ts";
import { armedId, focusedId, labelRev, renamingId, rev, rowHeight, scrollTop, segments, setFocusedId, setRenamingId, setScrollTop, setViewH, viewH, windowRows } from "../state/arrange.ts";

/** The parts of a track the rows read (a song store proxy). */
export interface Track { id: string; name: string; col: string; vol: number; mute?: boolean; solo?: boolean; bus?: boolean; open?: boolean; lanes: unknown[]; about?: unknown }
export interface TrackActions {
  devLabel(t: Track): string;
  about(t: Track): string;
  rename(t: Track, name: string): void;
  /** The lane element exists: register it and draw its clips; returns the cleanup. */
  mountLane(t: Track, el: HTMLElement): () => void;
  /** The open automation group exists: fill both columns' boxes; returns the cleanup. */
  mountAuto(t: Track, box: { h: HTMLElement; l: HTMLElement }): () => void;
}
interface Row { t: Track; box: { h: HTMLDivElement; l: HTMLDivElement }; inView: () => boolean }

// Filling a row (clips, automation lanes) costs DOM per note; rows entering while scrolling fill from a queue with a
// per-frame budget (they mount one screen ahead of the view, so the delay is not seen). Loads, edits and undo fill at
// once. A row mounted only because it is selected or focused (outside the view window) is not filled.
let deferring = false, raf = 0;
const queue = new Set<() => void>();
function fillLater(fn: () => void): () => void {
  queue.add(fn); if (!raf) raf = requestAnimationFrame(drain);
  return () => queue.delete(fn);
}
function drain() {
  raf = 0; const t0 = performance.now();
  for (const fn of queue) { queue.delete(fn); fn(); if (performance.now() - t0 > 6) break; }
  if (queue.size) raf = requestAnimationFrame(drain);
}
/** Runs `fill` (returns its cleanup) now, or from the queue while scrolling; cleans up with the owner. */
function fillRow(fill: () => () => void): void {
  let stop: (() => void) | undefined;
  if (deferring) { const cancel = fillLater(() => { stop = fill(); }); onCleanup(() => { cancel(); stop?.(); }); }
  else { stop = fill(); onCleanup(() => stop!()); }
}

const isOpen = (t: Track) => t.open ?? t.lanes.length > 0;

export function mountTracks(hdrs: HTMLElement, lanes: HTMLElement, scroller: HTMLElement, tracks: Track[], a: TrackActions): () => void {
  const lanesRows = document.createElement("div"); lanesRows.className = "rows"; lanes.prepend(lanesRows);
  hdrs.replaceChildren();
  setScrollTop(scroller.scrollTop); setViewH(scroller.clientHeight);
  const onScroll = () => { deferring = true; try { setScrollTop(scroller.scrollTop); } finally { deferring = false; } };
  scroller.addEventListener("scroll", onScroll, { passive: true });
  const ro = new ResizeObserver(() => setViewH(scroller.clientHeight)); ro.observe(scroller);
  const focusIn = (e: FocusEvent) => { const h = (e.target as HTMLElement).closest?.<HTMLElement>(".thdr.track"); if (h) setFocusedId(h.dataset.track!); };
  const focusOut = (e: FocusEvent) => { if (!(e.relatedTarget as HTMLElement | null)?.closest?.(".thdr.track")) setFocusedId(null); };
  hdrs.addEventListener("focusin", focusIn); hdrs.addEventListener("focusout", focusOut);

  const dispose = render(() => {
    const heights = createMemo(() => tracks.map((t) => rowHeight(isOpen(t), t.lanes.length)));
    const rows = createMemo(() => {
      const pin = [selectedId(), focusedId()].map((id) => tracks.findIndex((t) => t.id === id));
      return windowRows(heights(), scrollTop(), viewH(), pin);
    }, undefined, { equals: (x, y) => x.length === y.length && x.every((v, i) => v === y[i]) });
    const inWin = createMemo(() => new Set(windowRows(heights(), scrollTop(), viewH(), []).map((i) => tracks[i]?.id)));
    const shown = createMemo(() => rows().map((i) => tracks[i]));
    const rowOf = mapArray(shown, (t): Row => {
      const mk = () => { const d = document.createElement("div"); d.className = "lanegrp"; return d; };
      return { t, box: { h: mk(), l: mk() }, inView: createMemo(() => inWin().has(t.id)) };
    });
    const segs = createMemo(() => {
      const objs = rowOf(), byIdx = new Map(rows().map((ri, k) => [ri, objs[k]]));
      return segments(heights(), rows()).map((s) => (typeof s === "number" ? byIdx.get(s)! : s));
    });
    const gap = (g: { gap: number }) => <div class="gap" aria-hidden="true" style={{ height: `${g.gap}px` }} />;
    // the lanes column: same segments, its own container, this reactive owner
    insert(lanesRows, () => <For each={segs()}>{(s) => ("gap" in s ? gap(s) : <LaneRow r={s} a={a} />)}</For>);
    return <For each={segs()}>{(s) => ("gap" in s ? gap(s) : <HeaderRow r={s} a={a} tracks={tracks} />)}</For>;
  }, hdrs);
  return () => { dispose(); ro.disconnect(); scroller.removeEventListener("scroll", onScroll); lanesRows.remove(); hdrs.removeEventListener("focusin", focusIn); hdrs.removeEventListener("focusout", focusOut); };
}

function HeaderRow(p: { r: Row; a: TrackActions; tracks: Track[] }): JSX.Element {
  const t = p.r.t, a = p.a;
  const sel = () => selectedId() === t.id;
  const n = () => t.lanes.length;
  // names must be unique and non-empty (song validation): the header shows the problem at once
  const bad = createMemo(() => !t.name ? "name is empty" : p.tracks.filter((x) => x.name === t.name).length > 1 ? `name "${t.name}" is used twice` : "");
  const label = createMemo(() => { labelRev(); return a.devLabel(t); });
  return <>
    <div class="thdr track" classList={{ sel: sel(), bus: !!t.bus, err: !!bad() }} data-track={t.id} tabIndex={0} role="group" aria-label={`${t.name} track`}
      aria-current={sel() ? "true" : undefined} title="Double-click the name to rename · right-click (or Shift+F10) for the track menu">
      <span class="col" style={{ background: `var(${t.col})` }} />
      <div>
        <div class="name" title={bad() || a.about(t) || undefined}>
          <Show when={renamingId() === t.id} fallback={t.name}><Rename t={t} a={a} /></Show>
        </div>
        <div class="dev">{label()}</div>
      </div>
      <div class="btns">
        <button aria-pressed={!!t.mute} aria-label={`Mute ${t.name}`}>M</button>
        <button aria-pressed={!!t.solo} aria-label={`Solo ${t.name}`}>S</button>
        <Show when={!t.bus}><button class="arm" aria-pressed={armedId() === t.id} aria-label={`Arm ${t.name} for audition`} title="Arm: computer keyboard and MIDI play this track">●</button></Show>
        <button class="autobtn" aria-expanded={isOpen(t)} title="Automation lanes" onClick={() => { t.open = !isOpen(t); }}
          aria-label={n() ? `A${n()}: show ${n()} automation lanes of ${t.name}` : `A: add an automation lane to ${t.name}`}>A{n() || ""}</button>
      </div>
      <div class="vol">
        <input type="range" min="-60" max="6" step="0.1" value={t.vol} aria-label={`${t.name} volume`} aria-valuetext={`${t.vol} dB`} />
        <span class="num">{t.vol.toFixed(1)} dB</span>
      </div>
    </div>
    <Show when={isOpen(t)}><AutoGroup r={p.r} a={a} side="h" /></Show>
  </>;
}

function Rename(p: { t: Track; a: TrackActions }): JSX.Element {
  const old = untrack(() => p.t.name);
  let inp!: HTMLInputElement, done = false;
  const finish = (ok: boolean) => {
    if (done) return; done = true;
    const v = inp.value.trim();
    setRenamingId(null);
    if (ok && v !== old) p.a.rename(p.t, v);
  };
  onMount(() => { inp.focus(); inp.select(); });
  return <input ref={inp} value={old} aria-label={`Rename ${old}`}
    onKeyDown={(e) => { e.stopPropagation(); if (e.key === "Enter") finish(true); else if (e.key === "Escape") finish(false); }}
    onBlur={() => finish(true)} />;
}

/** One column's half of the open automation group; the lanes side fills both halves (they mount together). */
function AutoGroup(p: { r: Row; a: TrackActions; side: "h" | "l" }): JSX.Element {
  if (p.side === "l") {
    createEffect(on([rev, p.r.inView], ([, v]) => { if (v) fillRow(() => { const stop = p.a.mountAuto(p.r.t, p.r.box); return () => { stop(); p.r.box.h.replaceChildren(); p.r.box.l.replaceChildren(); }; }); }));
  }
  return p.r.box[p.side];
}

function LaneRow(p: { r: Row; a: TrackActions }): JSX.Element {
  const t = p.r.t;
  const bus = untrack(() => !!t.bus);  // static: the lane's other children are clips drawn imperatively
  let el!: HTMLDivElement;
  createEffect(on([rev, p.r.inView], ([, v]) => { if (v) fillRow(() => p.a.mountLane(t, el)); }));
  return <>
    <div ref={el} class="lane track" classList={{ sel: selectedId() === t.id, buslane: bus }} data-track={t.id} aria-label={`${t.name} clips`}
      style={{ opacity: t.mute ? 0.45 : undefined }}>
      {bus ? <span class="busnote">bus track · no clips · audio from sends</span> : null}
    </div>
    <Show when={isOpen(t)}><AutoGroup r={p.r} a={p.a} side="l" /></Show>
  </>;
}
