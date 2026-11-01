// Track headers and lanes (change daw-solid-ts 5.2, design D6): one row per track in two columns (#hdrs, #lanes),
// windowed by ui/state/arrange.ts (rows in view ± one screen, the selected and focused row always), spacers keep
// offsets and the scroll extent. Same classes, ids, data-track and ARIA as the old rows. Headers are reactive on the
// song store (name, mute/solo, volume, lanes, open) and the view state (selected, armed, renaming). Clips and
// automation lanes are still filled imperatively when a row mounts (`mountLane`, `mountAuto`; refilled on `rev`);
// Controls and header events call TrackActions (design D7; no document delegates).
import { batch, createEffect, createMemo, createRoot, For, mapArray, on, onCleanup, onMount, Show, untrack, type JSX } from "solid-js";
import { insert, render } from "solid-js/web";
import { laneEl, selectedId } from "../state/view.ts";
import { Clip, type PatternOf } from "./Clip.tsx";
import { armedId, barWindow, bw, focusedId, labelRev, renamingId, rev, rowHeight, scrollLeft, scrollTop, segments, setFocusedId, setRenamingId, setScrollLeft, setScrollTop, setViewH, setViewW, viewH, viewW, windowRows } from "../state/arrange.ts";

/** The parts of a track the rows read (a song store proxy). */
export interface Track { id: string; name: string; col: string; vol: number; mute?: boolean; solo?: boolean; bus?: boolean; open?: boolean; lanes: unknown[]; about?: unknown; clips: { name: string; start: number; len: number; loop?: boolean; sel?: boolean }[] }
export interface TrackActions {
  devLabel(t: Track): string;
  about(t: Track): string;
  rename(t: Track, name: string): void;
  /** A clip's pattern for its thumbnail (read only). */
  pattern: PatternOf;
  /** One automation lane's header and lane elements (imperative SVG + point editor, D4) and their dispose. */
  autoLane(t: Track, ln: unknown): { ah: HTMLElement; al: HTMLElement; dispose(): void };
  /** "+ lane": adds the next free parameter's lane to the track. */
  addAutoLane(t: Track): void;
  mute(t: Track): void;
  solo(t: Track): void;
  arm(t: Track): void;
  /** Live while the fader moves; `done` at the end of the gesture (one undo step). */
  volume(t: Track, db: number, done: boolean): void;
  /** Header events (selection, keys, menu); the header passes itself for keys aimed at it. */
  click(t: Track, e: MouseEvent): void;
  key(t: Track, e: KeyboardEvent, header: HTMLElement): void;
  menu(t: Track, e: MouseEvent): void;
  startRename(t: Track): void;
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
/** Runs `draw` now, or from the queue while scrolling; returns a cancel. */
const later = (draw: () => void): (() => void) => { if (deferring) return fillLater(draw); draw(); return () => {}; };
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
  const onScroll = () => { deferring = true; try { batchScroll(scroller); } finally { deferring = false; } };
  scroller.addEventListener("scroll", onScroll, { passive: true });
  setScrollLeft(scroller.scrollLeft); setViewW(scroller.clientWidth);
  const ro = new ResizeObserver(() => { setViewH(scroller.clientHeight); setViewW(scroller.clientWidth); }); ro.observe(scroller);
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

function batchScroll(el: HTMLElement) { batch(() => { setScrollTop(el.scrollTop); setScrollLeft(el.scrollLeft); }); }

function HeaderRow(p: { r: Row; a: TrackActions; tracks: Track[] }): JSX.Element {
  const t = p.r.t, a = p.a;
  const sel = () => selectedId() === t.id;
  const n = () => t.lanes.length;
  // names must be unique and non-empty (song validation): the header shows the problem at once
  const bad = createMemo(() => !t.name ? "name is empty" : p.tracks.filter((x) => x.name === t.name).length > 1 ? `name "${t.name}" is used twice` : "");
  const label = createMemo(() => { labelRev(); return a.devLabel(t); });
  let h!: HTMLDivElement;
  return <>
    <div ref={h} class="thdr track" classList={{ sel: sel(), bus: !!t.bus, err: !!bad() }}
      on:click={(e) => a.click(t, e)} on:keydown={(e) => a.key(t, e, h)} on:contextmenu={(e) => a.menu(t, e)} data-track={t.id} tabIndex={0} role="group" aria-label={`${t.name} track`}
      aria-current={sel() ? "true" : undefined} title="Double-click the name to rename · right-click (or Shift+F10) for the track menu">
      <span class="col" style={{ background: `var(${t.col})` }} />
      <div>
        <div class="name" title={bad() || a.about(t) || undefined} onDblClick={() => a.startRename(t)}>
          <Show when={renamingId() === t.id} fallback={t.name}><Rename t={t} a={a} /></Show>
        </div>
        <div class="dev">{label()}</div>
      </div>
      <div class="btns">
        <button aria-pressed={!!t.mute} aria-label={`Mute ${t.name}`} onClick={() => a.mute(t)}>M</button>
        <button aria-pressed={!!t.solo} aria-label={`Solo ${t.name}`} onClick={() => a.solo(t)}>S</button>
        <Show when={!t.bus}><button class="arm" aria-pressed={armedId() === t.id} aria-label={`Arm ${t.name} for audition`} title="Arm: computer keyboard and MIDI play this track" onClick={() => a.arm(t)}>●</button></Show>
        <button class="autobtn" aria-expanded={isOpen(t)} title="Automation lanes" onClick={() => { t.open = !isOpen(t); }}
          aria-label={n() ? `A${n()}: show ${n()} automation lanes of ${t.name}` : `A: add an automation lane to ${t.name}`}>A{n() || ""}</button>
      </div>
      <div class="vol">
        <input type="range" min="-60" max="6" step="0.1" value={t.vol} aria-label={`${t.name} volume`} aria-valuetext={`${t.vol.toFixed(1)} dB`}
          onInput={(e) => a.volume(t, +e.currentTarget.value, false)} onChange={(e) => a.volume(t, +e.currentTarget.value, true)} />
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

/** One column's half of the open automation group. The lanes side fills both halves: one header + lane pair per
 *  `t.lanes` entry (reactive list: adding or removing a lane needs no rebuild), then the "+ lane" row; refilled on
 *  `rev` (undo reconciles lanes by position, so their contents change in place). */
function AutoGroup(p: { r: Row; a: TrackActions; side: "h" | "l" }): JSX.Element {
  if (p.side === "l") {
    const { t, box } = p.r, a = p.a;
    createEffect(on([rev, p.r.inView], ([, v]) => {
      if (v) fillRow(() => createRoot((dispose) => {
        const pairs = mapArray(() => t.lanes, (ln) => { const r = a.autoLane(t, ln); onCleanup(r.dispose); return r; });
        insert(box.h, () => <>
          <For each={pairs()}>{(r) => r.ah}</For>
          <div class="thdr addlane"><button aria-label={`Add automation lane to ${t.name}`} onClick={() => a.addAutoLane(t)}>+ lane</button><span class="dim">one lane per parameter</span></div>
        </>);
        insert(box.l, () => <><For each={pairs()}>{(r) => r.al}</For><div class="lane addlane" /></>);
        return () => { dispose(); box.h.replaceChildren(); box.l.replaceChildren(); };
      }));
    }));
  }
  return p.r.box[p.side];
}

function LaneRow(p: { r: Row; a: TrackActions }): JSX.Element {
  const t = p.r.t;
  const bus = untrack(() => !!t.bus);
  // clips in the bars around the view, only while the row is in the view window (a pinned-only row is empty)
  const win = createMemo(() => barWindow(scrollLeft(), viewW(), bw()), undefined, { equals: (x, y) => x[0] === y[0] && x[1] === y[1] });
  const clips = createMemo(() => { if (!p.r.inView()) return []; const [b0, b1] = win(); return t.clips.filter((c) => c.start + c.len > b0 && c.start < b1); });
  const reg = (el: HTMLDivElement) => { laneEl.set(t.id, el); onCleanup(() => { if (laneEl.get(t.id) === el) laneEl.delete(t.id); }); };
  return <>
    <div ref={reg} class="lane track" classList={{ sel: selectedId() === t.id, buslane: bus }} data-track={t.id} aria-label={`${t.name} clips`}
      style={{ opacity: t.mute ? 0.45 : undefined }}>
      {bus ? <span class="busnote">bus track · no clips · audio from sends</span> : null}
      <For each={clips()}>{(c) => <Clip t={t} c={c} pattern={p.a.pattern} later={later} />}</For>
    </div>
    <Show when={isOpen(t)}><AutoGroup r={p.r} a={p.a} side="l" /></Show>
  </>;
}
