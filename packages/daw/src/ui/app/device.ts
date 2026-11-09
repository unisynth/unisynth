// Device commands of the running session (change daw-solid-ts 9.1, step 7): parameter edits to the engine, slot clear /
// filter swap (mod targets and lanes follow), effect chain edits, GSM sample import and cleanup, preset save, the
// device panel's actions (ui/device/Device.tsx) and the live per-voice watch. Screens: ./screens.ts; sources: ./samples.ts.
import type { Device, UITrack } from "../../model.ts";
import type { DeviceActions, Device as PanelDevice, Param as PanelParam, Track } from "../device/Device.tsx";
import type { Param } from "../../engine.ts";
import { mountDevice } from "../device/Device.tsx";
import * as live1 from "../device/live.ts";
import { openLibrary } from "../library/Library.tsx";
import * as arrange from "../state/arrange.ts";
import * as devst from "../state/device.ts";
import * as lib from "../state/library.ts";
import { persist as store } from "../state/persist.ts";
import { overrides } from "../state/view.ts";
import { choice, devName, effectChoices, fmtVal, ip, loadTime, modSrcName, nModes, optValue, oscType, paramOf, routesOf, SLOTS, type SlotGroup, slotKeys, slotUsed, smpShapes, tableOf, TARGETS, vget } from "../model/params.ts";
import { engineDev, instOf, links } from "../model/track.ts";
import { bytesToB64 } from "../model/bytes.ts";
import { api, E, selected, tracks } from "./session.ts";
import { commit, H, say, snap } from "./core.ts";
import { playing } from "./transport.ts";
import { livePush } from "./engine.ts";
import { applyAutomation, curVal, grab, knobEdited, recOn, recordPoint, release, updateReauto } from "./automation.ts";
import { rebuildTracks } from "./arrange.ts";
import { NAMES } from "./roll.ts";
import { envView, filtView, LFO_SHAPES, lfoView, oscView, yDb } from "./screens.ts";
import type { LibItem, Source } from "../state/library.ts";
import { gsmOf, gsmSlot, gsmUsed, NOBANK, pcmOf, playSample, smpWhat } from "./samples.ts";
import type { Snap } from "../state/snapshot.ts";

/** Makes the status line and the hidden audio-file input and starts the live per-voice view. Call once. */
export function initDevice(): void {
  status = document.createElement("div"); status.className = "rack-status"; status.setAttribute("aria-live", "polite");
  wavIn = Object.assign(document.createElement("input"), { type: "file", accept: "audio/*,.wav,.aif,.aiff,.flac,.mp3,.ogg" });
  wavIn.hidden = true; wavIn.setAttribute("aria-label", "Audio file to import as a GSM sample"); document.body.append(wavIn);
  wavIn.onchange = () => { const f = wavIn.files?.[0]; if (f && wavTarget) importAudio(f, wavTarget); };
  live1.initLive({ on: liveOn, inst: () => instOf(selected) ?? null, curVal, choice, lfoShape: (k) => LFO_SHAPES[k]!, yDb, noteName: (n) => NAMES[n]!, rate: () => E.rate });
}

/** Tracks that send to `t`. */
export const senders = (t: UITrack): UITrack[] => tracks.filter(x => x.sends.some(s1 => s1[0] === t.name));
/** Header device labels and the device panel. */
export function renderAll(): void {
  arrange.setLabelRev(v => v + 1);  // header device labels (fields like _edited are written outside the store)
  renderDevice();  // the mixer follows the song store by itself
}
// ---- Device panels generated from the engine's parameter tables (design D8). Values live in the song/2 device
// (`d.values`, by parameter name); a knob or select writes there, sets the live parameter at once and queues an
// engine update (structural parameters rebuild the device there). ----
/** The panel's status line (aria-live), shown under the mod matrix (made by initDevice). */
let status: HTMLElement = null as never;
/** Engine track index (the master is last). */
export const trackIndex = (t: UITrack): number => t.master ? tracks.length : tracks.indexOf(t);
/** The knob held now: a load-time value (a PAD table input) rebuilds the device once, when it is let go. */
let held: Param | null = null;
/** A parameter value to the engine now (load-time ones rebuild the device there: structural, PAD table inputs). */
function liveParam(t: UITrack, d: Device, p: Param, v: number): void {
  if (loadTime(d, p)) { if (held !== p) livePush(); return; }
  const di = engineDev(t, d); if (di >= 0 && E.node) E.setParam(trackIndex(t), di, p.index, v);
  livePush();
}
// Slot edits rewrite mod-matrix targets and automation lanes in one command (design D8a). `map(slot)` → new slot
// number or 0 (gone) for every slot of group g.
function remapSlots(t: UITrack, d: Device, g: SlotGroup, map: (slot: number) => number): void {
  const n = SLOTS[g], old: Record<number, Record<string, number>> = {};
  for (let i = 1; i <= n; i++) for (const k of slotKeys(g, i)) if (k in d.values) { (old[i] ??= {})[k.slice(k.indexOf("."))] = d.values[k]; delete d.values[k]; }
  for (let i = 1; i <= n; i++) { const j = map(i); if (j && old[i]) for (const [f, v] of Object.entries(old[i])) d.values[`${g}${j}${f}`] = v; }
  // a group's freed slots switch off when their type default is on
  for (let j = 1; j <= n; j++) { const ty = ip(`${g}${j}.type`); if (ty && ty.default !== 0 && !(`${g}${j}.type` in d.values) && ![...Array(n).keys()].some(i => map(i + 1) === j && old[i + 1])) d.values[`${g}${j}.type`] = 0; }
  // mod targets: osc2.gain → osc1.gain …; a target in a removed slot clears its mod slot
  const names = TARGETS(), re = new RegExp(`^${g}(\\d+)(\\..+)$`);
  for (let m = 1; m <= SLOTS.mod; m++) {
    const tk = `mod${m}.target`; if (!(tk in d.values)) continue; const nm = names[Math.round(d.values[tk]! * 255)] || "", mm = nm.match(re); if (!mm) continue;
    const j = map(+mm[1]); if (!j) { d.values[`mod${m}.source`] = 0; continue; }
    const id = names.indexOf(`${g}${j}${mm[2]}`); if (id >= 0) d.values[tk] = id / 255;
  }
  const di = t.devices.indexOf(d), lre = new RegExp(`^${g}(\\d+)(\\..+)$`);
  t.lanes = t.lanes.filter(ln => { if (ln.dev !== di) return true; const mm = ln.param.match(lre); if (!mm) return true; const j = map(+mm[1]); if (!j) return false; ln.param = `${g}${j}${mm[2]}`; return true; });
}
// Fixed slot blocks (change add-device-live-view D1): every slot at its own position; a slot not set starts collapsed
// to its header (type control); ✕ clears in place; only filters swap (serial order). Slot collapse overrides live
// for the session (per track), block collapse in `store` (editor state, never in the song).
function slotCmd(msg: string, fn: () => void): void { const before = snap(); fn(); rebuildTracks(); renderDevice(); status.textContent = msg; commit(before, msg); }
/** Adds (or reuses) a GSM sample in `d` and points sampler `slot` at it: one undo step. */
export function useGsm(t: UITrack, d: Device, b64: string, label: string, slot = gsmSlot(d)): void {
  const before = snap(); d.samples ??= [];
  let k = d.samples.indexOf(b64); if (k < 0) { d.samples.push(b64); k = d.samples.length - 1; }
  Object.assign(d.values, { [`osc${slot}.type`]: optValue(`osc${slot}.type`, "Sampler"), [`osc${slot}.s7`]: 0, [`osc${slot}.sample`]: k + 1 });
  overrides.get(t.id)?.clear(); livePush(); rebuildTracks(); renderAll(); commit(before, `${t.name}: osc ${slot} plays ${label} (GSM sample ${k + 1})`);
}
// Samples no GSM sampler slot plays still cost bytes in the intro: drop them and renumber the slots.
function removeUnusedGsm(t: UITrack, d: Device): void {
  const used = gsmUsed(d), before = snap(), keep: string[] = [], map = new Map<number, number>();
  (d.samples || []).forEach((b, k) => { if (used.has(k + 1)) { keep.push(b); map.set(k + 1, keep.length); } });
  const n = (d.samples || []).length - keep.length; d.samples = keep;
  for (const i of [1, 2, 3]) if (oscType(d, i) === 4 && choice(vget(d, ip(`osc${i}.s7`)), 3) === 0) d.values[`osc${i}.sample`] = map.get(Math.round(vget(d, ip(`osc${i}.sample`)))) ?? 0;
  livePush(); renderDevice(); commit(before, `${t.name}: removed ${n} unused GSM sample${n === 1 ? "" : "s"}`);
}
let wavIn: HTMLInputElement = null as never;  // hidden file input (made by initDevice)
type WavTarget = { t: UITrack; d: Device; slot: number };
let wavTarget: WavTarget | null = null;
function importWav(t: UITrack, d: Device, slot: number): void { wavTarget = { t, d, slot }; wavIn.value = ""; wavIn.click(); }
const GSM_MAX_SECONDS = 30;
async function importAudio(file: File, tg: WavTarget): Promise<void> {
  const ep = H.epoch;  // a song opened while decoding: the target track is gone
  try {
    const ab = await new OfflineAudioContext(1, 1, 44100).decodeAudioData(await file.arrayBuffer());  // resampled to 44.1 kHz
    if (H.epoch !== ep) return;
    const n = ab.length, mono = new Float32Array(n);
    for (let c = 0; c < ab.numberOfChannels; c++) { const ch = ab.getChannelData(c); for (let i = 0; i < n; i++) mono[i] += ch[i] / ab.numberOfChannels; }
    if (n > GSM_MAX_SECONDS * 44100) { say(`${file.name}: ${(n / 44100).toFixed(1)} s is too long for a 64k intro (at most ${GSM_MAX_SECONDS} s); trim it first`); return; }
    let peak = 0; for (const x of mono) peak = Math.max(peak, Math.abs(x));
    const chunk = E.gsmEncode(mono);
    useGsm(tg.t, tg.d, bytesToB64(chunk), file.name, tg.slot);
    say(`${file.name}: ${(n / 44100).toFixed(2)} s → ${(chunk.length / 1024).toFixed(1)} KB GSM${peak > 1 ? " · clipped: the file peaks above 0 dBFS" : ""}`);
  } catch (e) { say(`${file.name}: cannot read this audio file (${(e as Error)?.message || e})`); }
}
// Effect chain edits keep automation lanes (device index) in step with t.devices.
export function devBypass(t: UITrack, i: number): void { const d = t.devices[i], before = snap(); d.bypass = !d.bypass; renderAll(); rebuildTracks(); commit(before, `${t.name}: ${devName(d)} ${d.bypass ? "bypassed" : "on"}`); }
function moveDev(t: UITrack, i: number, dir: number): void {
  let j = i + dir; while (j >= 0 && j < t.devices.length && t.devices[j].kind === 64) j += dir;  // the instrument stays first
  if (j < 0 || j >= t.devices.length) return; const before = snap(), name = devName(t.devices[i]);
  [t.devices[i], t.devices[j]] = [t.devices[j], t.devices[i]];
  for (const ln of t.lanes) ln.dev = ln.dev === i ? j : ln.dev === j ? i : ln.dev;
  rebuildTracks(); renderAll(); commit(before, `${t.name}: ${name} moved ${dir < 0 ? "earlier" : "later"} (its automation lanes follow)`);
}
function removeDev(t: UITrack, i: number): void {
  const before = snap(), name = devName(t.devices[i]), gone = t.lanes.filter(l => l.dev === i).length;
  t.devices.splice(i, 1); t.lanes = t.lanes.filter(l => l.dev !== i); t.lanes.forEach(l => { if (l.dev > i) l.dev--; });
  rebuildTracks(); renderAll(); commit(before, `${t.name}: ${name} removed${gone ? `, with ${gone} automation lane${gone > 1 ? "s" : ""}` : ""}`);
}
export function addEffect(t: UITrack, label: string): void {
  const tb = effectChoices().find(x => x.label === label); if (!tb) return; const before = snap(), n = nModes(tb.kind);
  t.devices.push({ kind: tb.kind, values: { mode: n > 1 ? tb.mode / (n - 1) : 0 } }); rebuildTracks(); renderAll(); commit(before, `${t.name}: ${label} added`);
}
function savePreset(d: Device, btn: HTMLElement): void {
  const pop = document.createElement("div"); pop.className = "menu"; pop.setAttribute("role", "dialog"); pop.setAttribute("aria-label", "Save preset");
  pop.innerHTML = `<label class="dim" style="padding:4px 6px">Preset name<input class="pname" value="${(d.preset || "my patch").replace(/^GM /, "")}" style="width:100%"></label>
    <div class="dim small" style="padding:0 6px 4px">→ project/music/patches/<b class="pfile"></b></div><div style="display:flex;gap:4px;padding:4px 6px"><button class="ok">Save</button><button class="no">Cancel</button></div><div class="perr dim small" role="alert"></div>`;
  document.body.append(pop); const r = btn.getBoundingClientRect(); pop.style.left = Math.min(r.left, innerWidth - 260) + "px"; pop.style.top = r.bottom + 4 + "px";
  const q = <E extends HTMLElement>(sel: string): E => pop.querySelector<E>(sel)!;
  const inp = q<HTMLInputElement>(".pname"), file = (): string => inp.value.trim().toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-|-$/g, "") + ".patch.json";
  const upd = (): void => { q(".pfile").textContent = file(); q<HTMLButtonElement>(".ok").disabled = !inp.value.trim(); }; upd(); inp.oninput = upd; inp.select(); inp.focus();
  const close = () => { pop.remove(); btn.focus(); };
  q(".no").onclick = close;
  q(".ok").onclick = async () => {
    const nm = inp.value.trim(), path = `project/music/patches/${file()}`;
    const { _edited, ...dev } = d as Device & { _edited?: boolean }; void _edited; const text = E.presetSave({ ...dev, values: { ...d.values } }, nm);
    const ep = H.epoch, res = await api.write(path, text);
    if (H.epoch !== ep) { close(); return; }  // another song was opened meanwhile: the file is saved, the device is gone
    if (!res.ok) { q(".perr").textContent = res.errors?.map((e: { path: string; message: string }) => `${e.path}: ${e.message}`).join("; ") || res.error || ""; return; }
    const before = snap(); d.preset = nm; (d as Device & { _edited?: boolean })._edited = false; close(); lib.setCatalog(await api.library() || lib.catalog()); renderAll(); btn.focus(); commit(before, `saved preset ${nm} → ${path}`);
  };
  pop.addEventListener("keydown", e => { if (e.key === "Escape") close(); if (e.key === "Enter") q(".ok").click(); });
}
let devMounted = false;
/** Shows the selected track in the device panel (mounted at the first call) and the context line. */
export function renderDevice(): void {
  const t = selected;
  if (!devMounted) { devMounted = true; mountDevice(document.getElementById("device")!, deviceActions()); }
  devst.setShownTrack(t);
  const clip = t.clips?.find(c => c.sel) || t.clips?.[0];
  document.getElementById("ctx")!.innerHTML = `<span class="dim">Track</span> <b style="color:var(${t.col})">${t.name}</b>` +
    (clip ? ` <span class="dim">· clip</span> <b>${clip.name}</b>${links(t, clip.name) > 1 ? ` <span class="badge">⧉ ${links(t, clip.name)} linked</span>` : ""}` : "") +
    ` <span class="dim">· ${t.devices.length} device${t.devices.length === 1 ? "" : "s"}</span>`;
  applyAutomation(); updateReauto(t);
  updateWatch(); live1.markLiveDirty();
}

// ---- Live per-voice displays: ui/device/live.ts (probe parsing, spectra, overlays, chips; change add-device-live-view D7). ----
function liveOn(): boolean { return store.get("daw.live") !== "0"; }
// The device panel's actions (ui/device/Device.tsx; called once, at the first renderDevice).
/** DeviceActions as app code sees them: the panel's narrow `Track` / `Device` / `Param` are the store tracks, devices and
 * engine parameters it was given, handed back. */
type Real<T> = T extends Track ? UITrack : T extends PanelDevice ? Device : T extends PanelParam ? Param : T;
type Actions = { [K in keyof DeviceActions]: DeviceActions[K] extends (...a: infer P) => infer R ? (...a: { [I in keyof P]: Real<P[I]> }) => R : DeviceActions[K] };
function deviceActions(): DeviceActions {
  const a: Actions = {
    ip, tableOf, nModes, devName, fmtVal, vget, choice, oscType, smpShapes, slotUsed, routesOf, modSrcName,
    senders: t => senders(t).map(x => x.name),
    sideSenders: t => tracks.filter(x => x.sends.some(s1 => s1[0] === t.name && s1[1] === "side")).map(x => x.name),
    setParam: (t, d, p, v, msg) => { const before = snap(); d.values[p.name] = v; liveParam(t, d, p, v); if (p.structural || p.name.endsWith(".type") || p.name === "mode") renderDevice(); commit(before, msg); },
    knobInput: (t, d, p, v) => { d.values[p.name] = v; if (!(recOn && playing && recordPoint(t, d, p, v))) knobEdited(t, d, p); liveParam(t, d, p, v); },
    grab: (t, d, p) => { held = p; grab(t, d, p); },
    release: (t, d, p) => { held = null; release(t, d, p); if (loadTime(d, p)) livePush(); },
    begin: () => snap(), end: (before, msg) => commit(before as Snap, msg),
    // a mode change selects another parameter set: keep only the mode
    modeChanged: (t, d) => { for (const k of Object.keys(d.values)) if (k !== "mode") delete d.values[k]; renderDevice(); livePush(); },
    clearSlot: (t, d, g, i) => slotCmd(`${SLOT_TITLE(g, i)} cleared; mod rows that targeted it are off, its lanes removed`, () => remapSlots(t, d, g, y => y === i ? 0 : y)),
    swapFilters: (t, d, i) => slotCmd(`Filter ${i - 1} ↔ ${i}: mod targets and automation lanes follow`, () => remapSlots(t, d, "filt", x => x === i ? i - 1 : x === i - 1 ? i : x)),
    bypass: (t, i) => devBypass(t, i), move: (t, i, dir) => moveDev(t, i, dir), remove: (t, i) => removeDev(t, i),
    effects: () => effectChoices().map(tb => tb.label), addEffect: (t, label) => addEffect(t, label),
    browse: () => openLibrary(), importSample: (t, d) => importWav(t, d, gsmSlot(d)), savePreset: (_t, d, btn) => savePreset(d, btn),
    liveOn, toggleLive: () => { store.set("daw.live", liveOn() ? "0" : "1"); updateWatch(); },
    screen: (_t, d, g, i) => g === "env" ? envView(d, i) : g === "filt" ? filtView(d, i) : g === "lfo" ? lfoView(d, i) : oscView(d, i),
    // sampler block (ui/device/Sampler.tsx)
    curVal, smpWhat, gmMissing: () => !lib.catalog()?.gm || NOBANK,
    gsmUnused: d => { const n = d.samples?.length ?? 0, used = gsmUsed(d); return n > used.size ? n - [...used].filter(k => k >= 1 && k <= n).length : 0; },
    playSample: b64 => playSample(b64), openLibrary: src => openLibrary(src), importWav: (t, d, i) => importWav(t, d, i),
    removeUnusedGsm: (t, d) => removeUnusedGsm(t, d), pcm: pcmOf,
    put: (t, d, p, v) => { d.values[p.name] = v; liveParam(t, d, p, v); },
    dragEnd: (_t, d, before, msg) => { (d as Device & { _edited?: boolean })._edited = true; commit(before as Snap, msg); },
    status,
  };
  return a as unknown as DeviceActions;
}
const SLOT_TITLE = (g: SlotGroup, i: number): string => `${({ osc: "Osc", filt: "Filter", env: "Env", lfo: "LFO" } as Record<string, string>)[g]} ${i}`;  // live view on/off, remembered in the browser
/** The engine reports per-voice state only for the selected instrument track while the Device tab and Live are on. */
export function updateWatch(): void {
  const t = selected, on = liveOn() && !document.getElementById("p-dev")!.hidden && t && !t.bus && !t.master && instOf(t);
  E.watch(on ? trackIndex(t) : -1);
  if (!on) live1.liveClear();
}
/** Library "Load": a project preset, a converted legacy entry (its effects follow the instrument), a GSM sample or a
 * GM.DLS instrument on a sampler slot replaces the selected track's sound. One undo step. */
export async function loadInto(t: UITrack, src: Source, it: LibItem): Promise<void> {
  const before = snap(); let inst = instOf(t); if (!inst) return;
  if (src.id === "project") {
    const text = await api.read(it.preset.path), r = E.presetLoad(text);
    if (before.epoch !== H.epoch) return;  // another song was opened meanwhile
    if (!r.device) { say(`${it.name}: ${r.errors?.map((e: { message: string }) => e.message).join("; ")}`); return; }
    const i = t.devices.indexOf(inst);
    const lost = t.lanes.filter(ln => ln.dev === i && !paramOf(r.device as Device, ln.param)).map(ln => ln.param);
    if (lost.length) { say(`${it.name}: the lanes on ${lost.join(", ")} have no parameter in this patch`); return; }
    t.devices[i] = { ...r.device, _edited: false } as Device; inst = t.devices[i]!;
  } else if (it.conv?.devices?.length) {
    // converted legacy entry: its instrument replaces the track's, its insert effects go right after it
    const [ins, ...fx] = (it.conv.devices as Device[]).map(d => ({ ...structuredClone(d), preset: it.name, _edited: false }));
    const i = t.devices.indexOf(inst);
    const lost = t.lanes.filter(ln => ln.dev === i && !paramOf(ins!, ln.param)).map(ln => ln.param);
    if (lost.length) { say(`${it.name}: the lanes on ${lost.join(", ")} have no parameter in this patch`); return; }
    t.devices.splice(i, 1, ins!, ...fx); inst = ins!;
    for (const ln of t.lanes) if (ln.dev > i) ln.dev += fx.length;
  } else if (src.id === "gsm") {
    const b64 = await gsmOf(it.gsm); if (before.epoch !== H.epoch) return; if (!b64) { say(`${it.name}: sample not found in ${it.gsm.path}`); return; }
    useGsm(t, inst, b64, it.name); return;
  } else if (src.id === "gm") {
    // a sampler slot on the GM.DLS instrument (source 0.5): each note plays the region for its key and velocity
    const d = inst, g = it.gm, slot = [1, 2, 3].find(i => oscType(d, i) === 4) || [1, 2, 3].find(i => !slotUsed(d, "osc", i)) || 1;
    Object.assign(d.values, { [`osc${slot}.type`]: optValue(`osc${slot}.type`, "Sampler"), [`osc${slot}.s7`]: 0.5, [`osc${slot}.sample`]: 1 + (g.drums ? 16384 : 0) + g.bank * 128 + g.prog });
    delete d.values[`osc${slot}.s8`];
    d.preset = `GM ${it.gm.name}`; (d as Device & { _edited?: boolean })._edited = false;
  }
  overrides.get(t.id)?.clear(); rebuildTracks(); renderAll(); say(`${t.name}: loaded “${it.name}” (one undo step)`); commit(before, `${t.name}: loaded ${it.name}`);
}
