// Transport state (change daw-solid-ts, design D3): what the transport bar shows and edits. the session (ui/app/transport.ts) publishes its
// transport variables here (loop, bars, follow, record, edited, drops) where it refreshes their display, and reads the
// fields the bar owns (tempo, signature, snap, buffer, open filters) from here. Song tempo/signature are not undoable.
import { createSignal } from "solid-js";

export const [tempo, setTempo] = createSignal(128);
export const [sig, setSig] = createSignal("4/4");
export const [sigOptions, setSigOptions] = createSignal(["4/4", "3/4", "7/8"]);
export const SNAPS = ["1/16", "1/8", "1/4", "Bar", "Off"] as const;
export const [snap, setSnap] = createSignal<string>("1/16");
/** Song length in bars. */
export const [bars, setBars] = createSignal(32);
/** Loop range [start, end) in bars and whether it is on. */
export const [loop, setLoop] = createSignal<[number, number]>([1, 5], { equals: (a, b) => a[0] === b[0] && a[1] === b[1] });
export const [loopOn, setLoopOn] = createSignal(false);
export const [follow, setFollow] = createSignal(true);
/** Follow paused by a hand scroll (resumes at the right edge, on click or F). */
export const [followHeld, setFollowHeld] = createSignal(false);
export const [rec, setRec] = createSignal(false);
/** An edit happened since play started. */
export const [edited, setEdited] = createSignal(false);
/** Audio drops since Play; null when the browser does not report them. */
export const [drops, setDrops] = createSignal<{ n: number; title: string } | null>(null, { equals: (a, b) => a?.n === b?.n && a?.title === b?.title });
export const BUFFERS: [string, string][] = [["interactive", "Low · ~5 ms"], ["balanced", "Normal · ~10 ms"], ["playback", "Safe · ~23 ms"], ["0.05", "Max · ~50 ms"]];
export const [buffer, setBuffer] = createSignal("playback");
export const [openFilters, setOpenFilters] = createSignal(false);

/** Selects a signature, adding it to the choices when a song uses an unlisted one. */
export function ensureSig(v: string): void {
  if (!sigOptions().includes(v)) setSigOptions([...sigOptions(), v]);
  setSig(v);
}

const parts = (s: string) => s.split("/").map(Number) as [number, number];
/** Quarter notes per bar (7/8 → 3.5). */
export const quartersPerBar = (s: string) => { const [n, d] = parts(s); return n * 4 / d; };

/** Position as bars.beats.ticks in the signature's unit (one quarter = 960 ticks); `b` in bars, 1-based. */
export function fmtPos(b: number, s: string): string {
  const [n, d] = parts(s), t = Math.max(0, b - 1) * n, beat = Math.floor(t);
  return `${1 + Math.floor(beat / n)}.${1 + beat % n}.${Math.floor((t - beat) * 960 * 4 / d)}`;
}

/** Time of a position, m:ss.mmm. */
export function fmtTime(b: number, bpm: number, s: string): string {
  const sec = (b - 1) / ((bpm || 128) / 60 / quartersPerBar(s));
  return `${Math.floor(sec / 60)}:${(sec % 60).toFixed(3).padStart(6, "0")}`;
}
