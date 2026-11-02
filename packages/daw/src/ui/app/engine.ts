// Engine link (change daw-solid-ts 9.1, steps 3–4; add-synth-daw D6/D8e): the AudioWorklet live host plays the
// song and the UI clock follows the position it reports. Every edit sends the exported song to the host, which
// applies it per track. Also: play/stop/seek, the per-frame clock while playing, meters, engine load and audio
// drops, the output buffer and the open-filter switch. `initEngine` takes what a seek and a frame must refresh in
// the views (arrangement, automation, recording).
import { audible as modelAudible, toSong, type Song, type UITrack } from "../../model.ts";
import { raw } from "../state/song.ts";
import * as tr from "../state/transport.ts";
import * as liveState from "../state/live.ts";
import { persist } from "../state/persist.ts";
import * as live1 from "../device/live.ts";
import { cleanDev, type UIDevice } from "../model/track.ts";
import { DAW, E, master, tracks } from "./session.ts";
import { say } from "./core.ts";
import { BARS, barOf, fmtPos, frameOf, LOOP, loopOn, PAD_BARS, playing, resumeFollow, setPlaying, setPos, sig, SONG_POS } from "./transport.ts";

export interface EngineHooks {
  /** Playhead readouts, playheads and follow scrolling for SONG_POS. */
  showPos(): void;
  /** After a seek: automated controls and lane readouts for the new position. */
  seeked(): void;
  /** Every animation frame while playing, after SONG_POS moved (recording, automation, lane highlights). */
  frame(now: number): void;
}
let hooks: EngineHooks = { showPos() {}, seeked() {}, frame() {} };

/** song/2 meta (title, author, licence notes) and [bars, ticks] as loaded: written back unchanged. */
let songMeta: Record<string, unknown> | undefined, songLen0: [number, number] | undefined;
export function setSongMeta(meta: typeof songMeta, len0: typeof songLen0): void { songMeta = meta; songLen0 = len0; }

/** Audible = not muted, and soloed, a send source of a soloed track, or a bus it sends to when any solo is on. */
export const audible = (t: UITrack): boolean => modelAudible(tracks, t);
const uiState = () => ({ tracks: raw(tracks), master: raw(master), tempo: tr.tempo() || 120, sig: sig(), bars: BARS, length0: songLen0, ...(songMeta ? { meta: songMeta } : {}) });
/** The song as song/2 (`live`: what the engine plays, mute/solo applied). */
export function songOf(live: boolean): Song {
  const s1 = toSong(uiState() as Parameters<typeof toSong>[0], live ? audible : undefined);
  for (const t of [...s1.tracks, s1.master]) t.devices = t.devices.map((d) => cleanDev(d as UIDevice));
  return s1;
}

let liveLoaded = false, pushT: ReturnType<typeof setTimeout> | undefined, pushing = false, statusAt = 0, lastIssue = "", raf = 0;
/** The engine must get the whole song again (new song, restarted audio). */
export function unloadLive(): void { liveLoaded = false; }
// One push at a time; a push asked for while one runs runs once more after it (never dropped: a Play or an edit
// during a long load still reaches the engine). Resolves when the engine has the current song.
let pushQ: Promise<void> = Promise.resolve(), pushAgain = false;
export function pushSong(): Promise<void> {
  if (pushing) { pushAgain = true; return pushQ; }
  pushQ = (async () => { do { pushAgain = false; await pushOnce(); } while (pushAgain && E.node); })();
  return pushQ;
}
async function pushOnce(): Promise<void> {
  if (!E.node) return; pushing = true;
  try {
    const r = E.exportBlob(JSON.stringify(songOf(true)));
    if (!r.blob) { const e = r.errors?.[0]; lastIssue = e ? `${e.path}: ${e.message}` : "export failed"; say(`engine not updated: ${lastIssue}`); return; }
    lastIssue = "";
    const res: any = await (liveLoaded ? E.update(r.blob) : E.load(r.blob));
    if (!res.ok) { say(`engine: ${res.reply?.errors?.[0]?.message || "update failed"}`); liveLoaded = false; return; }
    if (!liveLoaded) { liveLoaded = true; liveLoop(); }
    if (playing && liveLoaded) tr.setEdited(true);
  } finally { pushing = false; }
}
/** An edit reaches the engine 30 ms later (edits in a burst coalesce). */
export function livePush(): void { if (!E.node) return; clearTimeout(pushT); pushT = setTimeout(pushSong, 30); }
/** The loop range and state to the engine. */
export function liveLoop(): void { if (E.node && liveLoaded) E.setLoop(frameOf(LOOP[0]), frameOf(LOOP[1]), loopOn); }

// Run open filters (listening switch, remembered): live playback only, see Engine.runOpenFilters.
export function onOpenFilters(on: boolean): void { tr.setOpenFilters(on); persist.set("daw.openflt", on ? "1" : "0"); E.runOpenFilters(on); say(`open filters ${on ? "run (as WaveSabre)" : "skipped"} in live playback`); }
// Output buffer (Buffer select, remembered): applied by rebuilding the audio graph, keeping position and play state.
const bufHint = (v: string): any => (isNaN(+v) ? v : +v);
export async function onBuffer(v: string): Promise<void> {
  tr.setBuffer(v); persist.set("daw.buffer", v); E.latency = bufHint(v);
  if (!E.node) return;
  const was = playing; if (was) await setPlay(false);
  await E.restart(44100); liveLoaded = false;
  if (!(await ensureAudio())) return say(`cannot restart audio: ${lastIssue || "audio did not start"}`);
  E.seek(frameOf(SONG_POS));
  say(`output buffer ${Math.round(E.bufferSeconds * 1000)} ms`);
  if (was) await setPlay(true);
}
/** Starts (or resumes) audio and loads the song; false when it could not. */
export async function ensureAudio(): Promise<boolean> {
  if (!E.node || E.dead) { await E.start(44100); liveLoaded = false; }
  else await E.ctx!.resume();
  if (!liveLoaded) await pushSong();
  return liveLoaded;
}
export function seek(b1: number): void {
  resumeFollow(); setPos(Math.max(1, Math.min(BARS + PAD_BARS + 1, b1)));
  if (E.node) E.seek(frameOf(SONG_POS));
  statusAt = performance.now(); hooks.showPos(); hooks.seeked();
}
export async function setPlay(on: boolean): Promise<void> {
  if (on) {
    if (!(await ensureAudio())) return say(`cannot play: ${lastIssue || "audio did not start"}`);
    liveLoop(); E.play(frameOf(SONG_POS)); statusAt = performance.now(); dropsReset();
    setPlaying(true); resumeFollow(); tr.setEdited(false); raf = requestAnimationFrame(tick); say(`playing from ${fmtPos(SONG_POS)}`);
  } else {
    if (E.node) E.stop(); setPlaying(false); cancelAnimationFrame(raf); say(`stopped at ${fmtPos(SONG_POS)}`); setLoad(0);
  }
}
/** Stop: first press stops (tails ring), a second one silences and goes back to the loop or song start. */
export function stopPressed(): void {
  if (playing) { setPlay(false); return; }
  if (E.node) { E.stop(); E.seek(frameOf(loopOn ? LOOP[0] : 1)); }
  setPos(loopOn ? LOOP[0] : 1); hooks.showPos(); say(`back to ${fmtPos(SONG_POS)}`);
}
function tick(now: number): void {
  const st = E.status;
  // between status messages (about every 30 ms) the clock runs on from the last reported frame
  const f = st.position + (st.playing ? (now - statusAt) / 1000 * E.rate : 0);
  let b = barOf(f);
  if (loopOn && b >= LOOP[1] && barOf(st.position) < LOOP[1]) b = LOOP[0] + (b - LOOP[1]);
  if (!loopOn && b >= BARS + 1) { setPos(BARS + 1); hooks.showPos(); setPlay(false); return; }
  setPos(b); hooks.frame(now);
  raf = requestAnimationFrame(tick);
}

// Meters: per-track peaks from the engine (master last), falling 20 dB/s.
const lvl: Record<string, number[]> = {};
let loadShown = -1, loadAvg = 0, meterAt = performance.now();  // engine load: mean render time per real time, ~0.5 s average
function setLoad(v: number): void { if (v === loadShown) return; loadShown = v; liveState.setLoad(v); }
function meters(st: { peaks: ArrayLike<number> }): void {
  const now = performance.now(), dt = Math.min(.2, (now - meterAt) / 1000); meterAt = now;
  [...tracks, master].forEach((t, i) => {
    const id = t.id, pk = [st.peaks[2 * i] || 0, st.peaks[2 * i + 1] || 0], cur = lvl[id] || [0, 0];
    const v = pk.map((p, c) => { const x = Math.max(Math.min(1, p), cur[c]! * Math.pow(10, -dt)); return x < 1e-3 ? 0 : x; }); lvl[id] = v;
    liveState.setPeak(id, v[0]!, v[1]!);  // publishes only a visible change (liveState.PEAK_STEP)
  });
}
// Audio drops: the browser's count of output underruns (silence played because samples came late), counted from
// the last Play, with the song positions where new ones appeared. The first 1.5 s after Play are not counted: a
// just-started output often underruns once before any sound, and the browser updates the counter about once a second.
type Drops = { events: number; seconds: number };
let dropBase: Drops | null = null, dropSeen = 0, dropWhere: string[] = [], dropFrom = 0;
function dropsReset(): void { dropBase = E.drops(); dropSeen = 0; dropWhere = []; dropFrom = performance.now() + 1500; dropsShow(0, 0); }
function dropsCheck(): void {
  if (!playing) return;
  const d = E.drops(); if (!d) return;
  if (!dropBase || d.events < dropBase.events || performance.now() < dropFrom) dropBase = d;  // start-up; a new context starts from zero
  const n = d.events - dropBase.events;
  if (n > dropSeen) { dropWhere.push(fmtPos(SONG_POS)); if (dropWhere.length > 8) dropWhere.shift(); dropSeen = n; }
  dropsShow(n, d.seconds - dropBase.seconds);
}
function dropsShow(n: number, sec: number): void {
  if (!E.drops()) { tr.setDrops(null); return; }
  tr.setDrops({ n, title: n ? `Audio drops since Play: ${n}, ${Math.round(sec * 1000)} ms of silence, at ${dropWhere.join(", ")}${dropWhere.length >= 8 ? " (last 8)" : ""}. Raise Buffer, or close other heavy programs.`
    : "Audio drops since Play: none (the sound card always got its samples in time)" });
}

/** Wires the engine's callbacks and the remembered audio settings; call once, after initSession. */
export function initEngine(h: EngineHooks): void {
  hooks = h;
  const st0 = DAW.initial; setSongMeta(st0.meta, st0.length0);
  tr.setOpenFilters(persist.get("daw.openflt") === "1"); E.runOpen = tr.openFilters();
  { const v = persist.get("daw.buffer"); if (v && tr.BUFFERS.some(([x]) => x === v)) tr.setBuffer(v); E.latency = bufHint(tr.buffer()); }
  // A dead engine (trap in the worklet) must not leave a frozen readout: stop, zero the meters, say so; Play restarts it.
  E.onError = (msg: string) => {
    liveLoaded = false; if (playing) setPlay(false); setLoad(0); meters({ peaks: [] });
    say(`audio engine stopped (${msg}); press Play to restart it`);
  };
  E.onStatus = (st) => {
    statusAt = performance.now(); meters(st); dropsCheck(); live1.liveProbe(st.probe);
    loadAvg += (st.load - loadAvg) * .06; setLoad(Math.round(Math.min(1, st.load ? loadAvg : (loadAvg = 0)) * 100));
    if (!st.playing && playing && st.gen === E.gen) setPlay(false);
  };
}
