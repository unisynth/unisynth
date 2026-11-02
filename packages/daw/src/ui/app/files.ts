// Song files of the running session (change daw-solid-ts 9.1, step 8): the open song, saved / unsaved state, open,
// save (validated), new, import blob, export blob, bounce to WAV. Songs live in project/music (/api/music).
import type { UITrack } from "../../model.ts";
import * as tr from "../state/transport.ts";
import * as files from "../state/files.ts";
import * as dialogs from "../files/dialogs.tsx";
import { api, DAW, E, master, tracks } from "./session.ts";
import { H, restore, say, snap } from "./core.ts";
import { BARS, barsPerSec, beatsPerBar, fmtBars, LOOP, playing, setBars, setPos } from "./transport.ts";
import { livePush, setPlay, setSongMeta, songOf, unloadLive } from "./engine.ts";
import { fitSong, relayout, showPos, updateLoop } from "./arrange.ts";
import { initInstrument, selectTrack } from "./tracks.ts";

/** The song list from the server, the file main.ts opened, and the unsaved-changes guard on leaving. */
export function initFiles(): void {
  files.setSongs((DAW.files || []).filter((x) => x.kind === "song").map((x) => x.path).sort());
  songFile = DAW.file || songFile;
  window.addEventListener("beforeunload", e => { if (isDirty()) { e.preventDefault(); e.returnValue = ""; } });
}
/** The opened song counts as saved (startup). */
export function markClean(): void { markSaved(); savedAt = null; files.setSongFile(songFile); files.addSong(songFile); dirtyUpd(); }

/** The open song's path. */
export let songFile = "project/music/demo.song.json";
let savedId = -1, savedExtra = "", savedDepth = 0, savedAt: Date | null = null;
const base = (p: string): string => p.replace(/^.*\//, "");
// Unsaved = another committed state than at save (history state ids) or song settings that are not undoable changed.
const extraKey = (): string => tr.tempo() + "|" + tr.sig() + "|" + BARS;
const markSaved = (): void => { savedId = H.stateId; savedExtra = extraKey(); };
const isDirty = (): boolean => H.stateId !== savedId || extraKey() !== savedExtra;
/** Title, the songs list's unsaved mark and the saved-info line. */
export function dirtyUpd(): void {
  const d = isDirty(), n = Math.max(d ? 1 : 0, Math.abs(H.depth - savedDepth));
  document.title = `${d ? "• " : ""}${base(songFile)} — 64k DAW`;
  files.setDirty({ on: d, n });
  document.getElementById("saved-info")!.textContent = d ? `${n} unsaved change${n > 1 ? "s" : ""}` : savedAt ? `Saved ${savedAt.toTimeString().slice(0, 5)}` : "No changes";
}
// Puts a song/1 source into the editor (undo history cleared).
function useSong(s: unknown, file: string, saved: boolean): void {
  if (playing) setPlay(false);
  const st = DAW.model.fromSong(s as Parameters<typeof DAW.model.fromSong>[0]), d = JSON.parse(snap()); setSongMeta(st.meta, st.length0);
  d.tracks = st.tracks; d.sel = (st.tracks.find(t => !t.bus) || st.tracks[0])?.id ?? "master"; d.armed = st.tracks.find(t => t.devices.some(x => x.kind === 64))?.id;
  Object.keys(master).forEach(k => delete (master as Record<string, unknown>)[k]); Object.assign(master, st.master);
  tr.setTempo(st.tempo); const v = st.sig.join("/"); tr.ensureSig(v); document.documentElement.style.setProperty("--beats", String(beatsPerBar()));
  setBars(st.bars); LOOP[0] = 1; LOOP[1] = Math.min(BARS, 4) + 1;
  restore(JSON.stringify(d));
  H.clear(); songFile = file; files.setSongFile(file); files.addSong(file); savedDepth = 0; savedAt = saved ? new Date() : null;
  relayout(); updateLoop(); setPos(1); showPos(); fitSong(true); if (saved) markSaved(); else savedId = -1; unloadLive(); livePush(); dirtyUpd();
}
async function loadSong(f: string): Promise<void> {
  try { useSong(JSON.parse(await api.read(f)), f, true); say(`opened ${f}`); } catch (e) { say(`cannot open ${f}: ${(e as Error).message}`); }
}
/** Opens a song from the list (asks to save unsaved changes first). */
export function openSong(f: string): void {
  if (f === songFile) return;
  if (!isDirty()) { loadSong(f); return; }
  dialogs.confirmSave(songFile, "Your changes are lost if you switch without saving.", save, () => loadSong(f));
}
// Errors carry their JSON path (crates/song rules, also checked by the server before it writes).
function trackOfPath(path: string): UITrack | null { const m = path.match(/^\/tracks\/(\d+)/); return m ? tracks[+m[1]!] ?? null : path.startsWith("/master") ? master : null; }
type PathError = { path: string; message: string };
function showErrors(errs: PathError[], what: string): void {
  dialogs.showErrors(errs, what, path => { const t = trackOfPath(path); if (!t) return; selectTrack(t); document.querySelector<HTMLElement>(`.thdr.track[data-track="${t.id}"]`)?.focus(); });
}
function localIssues(): PathError[] {
  const errs: PathError[] = [], seen = new Map<string, number>();
  tracks.forEach((t, i) => { if (!t.name) errs.push({ path: `/tracks/${i}/name`, message: "track name is empty" }); else if (seen.has(t.name)) errs.push({ path: `/tracks/${i}/name`, message: `duplicate track name "${t.name}"` }); else seen.set(t.name, i); });
  return errs;
}
/** Validates (locally and by the engine) and writes the song; false when it did not save. */
export async function save(): Promise<boolean> {
  const json = DAW.model.prettyJson(songOf(false)), errs = [...localIssues(), ...E.validate(json)];
  if (errs.length) { showErrors(errs, "Save"); return false; }
  const r = await api.write(songFile, json);
  if (!r.ok) { if (r.errors) showErrors(r.errors, "Save"); else say(`save failed: ${r.error}`); return false; }
  markSaved(); savedDepth = H.depth; savedAt = new Date(); files.addSong(songFile);
  dirtyUpd(); say(`${api.static ? "downloaded (read-only preview)" : "saved"} ${songFile} (song/1, ${(json.length / 1024).toFixed(1)} kB)`); return true;
}
const emptySong = (): unknown => ({ format: "song/1", rate: 44100, tempo: 120, signature: [4, 4], ticks_per_beat: 960, length: 8 * 3840, clips: [],
  tracks: [{ id: "t1", name: "Track 1", color: "--t-lead", volume: 0.5, mute: false, receives: [], devices: [initInstrument()], placements: [], lanes: [] }],
  master: { id: "master", name: "Master", volume: 1, mute: false, receives: [{ from: "t1", channel: "main", volume: 1 }], devices: [], placements: [], lanes: [] } });
/** A new untitled song (asks to save unsaved changes first). */
export function newSong(): void {
  const go = (): void => { let n = 1; while (files.songs().includes(`project/music/untitled-${n}.song.json`)) n++; const f = `project/music/untitled-${n}.song.json`;
    useSong(emptySong(), f, false); say(`new song ${base(f)}: one instrument track and the master · double-click the lane to create a clip`); };
  if (!isDirty()) { go(); return; }
  dialogs.confirmSave(songFile, "Creating a new song closes this one.", save, go);
}
// Import blob: `song import` in the engine module (devices, routing and automation exact; each lane one clip).
/** Opens a song blob (`song import` in the engine) as an unsaved song. */
export async function importFile(file: File): Promise<void> {
  const r = E.importBlob(new Uint8Array(await file.arrayBuffer()));
  if (!r.json) { dialogs.notice(`Cannot import ${file.name}`, r.error ?? ""); return; }
  const f = `project/music/${file.name.replace(/\.(song\.)?bin$/i, "")}.song.json`;
  useSong(JSON.parse(r.json), f, false); say(`imported ${file.name} (${file.size} bytes) → ${base(f)} · save to keep it`);
}
// Export: the blob the intro player reads, written next to the song (same code as `song export`).
/** Export dialog: the blob the intro player reads, written next to the song. */
export async function exportSong(): Promise<void> {
  const errs = localIssues(); if (errs.length) { showErrors(errs, "Export"); return; }
  const s1 = songOf(false), r = E.exportBlob(JSON.stringify(s1)); if (!r.blob) { showErrors(r.errors ?? [], "Export"); return; }
  const bin = songFile.replace(/\.song\.json$/, ".song.bin"), notes = s1.tracks.filter((t) => !t.mute).reduce((n, t) => n + t.placements.reduce((k, pl) => k + (s1.clips.find((c) => c.id === pl.clip)?.notes.length || 0), 0), 0);
  const sec = BARS / barsPerSec(), all = [...s1.tracks, s1.master];
  dialogs.exportDialog({ bin, kb: (r.blob.length / 1024).toFixed(1), tracks: s1.tracks.length, notes,
    devices: all.reduce((n, t) => n + t.devices.filter(d => !d.bypass).length, 0), lanes: all.reduce((n, t) => n + t.lanes.length, 0),
    length: `${BARS} bars · ${Math.floor(sec / 60)}:${String(Math.round(sec % 60)).padStart(2, "0")} at ${tr.tempo()} BPM · 44.1 kHz` },
    async () => { const w = await api.write(bin, r.blob!); say(w.ok ? `exported ${bin} (${(r.blob!.length / 1024).toFixed(1)} kB)` : `export failed: ${w.error}`); },
    () => { const a = document.createElement("a"); a.href = URL.createObjectURL(new Blob([DAW.model.prettyJson(s1)], { type: "application/json" })); a.download = base(songFile); a.click(); });
}
// Bounce: the exported blob rendered by the song player in a Worker (design D7, equal to `song render`).
/** Bounce dialog: the exported blob rendered to a WAV (whole song or the loop). */
export function bounceSong(): void {
  const whole: [number, number] = [1, BARS + 1], fmt = (a: number, b1: number): string => { const sec = (b1 - a) / barsPerSec(); return `bars ${fmtBars(a)}–${fmtBars(b1)} · ${Math.floor(sec / 60)}:${String(Math.round(sec % 60)).padStart(2, "0")}`; };
  dialogs.bounceDialog({ song: fmt(...whole), loop: fmt(LOOP[0], LOOP[1]) }, (loop, progress) => {
    const [a, b1] = loop ? [LOOP[0], LOOP[1]] : whole;
    const r = E.exportBlob(JSON.stringify(songOf(false))); if (!r.blob) { showErrors(r.errors ?? [], "Bounce"); return null; }
    const rate = 44100, range: [number, number] | null = loop ? [Math.round((a - 1) / barsPerSec() * rate), Math.round((b1 - 1) / barsPerSec() * rate)] : null;
    const job = E.bounce(r.blob, range, progress);
    return { cancel: () => job.cancel(), done: job.done.then(samples => {
      const blob = DAW.wav16(samples, rate), name = base(songFile).replace(/\.song\.json$/, "") + (loop ? `-bars${fmtBars(a)}-${fmtBars(b1)}` : "") + ".wav";
      say(`bounced ${name}`); return { name, url: URL.createObjectURL(blob), seconds: samples.length / 2 / rate, mb: blob.size / 1048576 };
    }) };
  });
}
