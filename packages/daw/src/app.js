// @ts-nocheck
import { raw } from "./ui/state/song.ts";
import { api, armed, E, initSession, master, selected, song, tracks } from "./ui/app/session.ts";
import { commit, H, initCore, redo, restore, say, snap, undo } from "./ui/app/core.ts";
import { BARS, beatsPerBar, LOOP, loopOn, playing, setBars, sig } from "./ui/app/transport.ts";
import { addAutoLane, addLane, applyAutomation, initAutomation, laneVals, recOn, recordFrame, setRec } from "./ui/app/automation.ts";
import { BW, fitArr, fitSong, hdrs, initArrange, lanes, laneTick, loopSelection, lw, onLength, onLoopText, rebuildTracks, relayout, ruler, showPos, toggleFollow, toggleLoop, updateLoop, zoomArr } from "./ui/app/arrange.ts";
import { initClips } from "./ui/app/clips.ts";
import { addEffect, devBypass, initDevice, renderAll, renderDevice, senders, trackIndex, updateWatch, useGsm } from "./ui/app/device.ts";
import { gsmOf, playSample } from "./ui/app/samples.ts";
import { addTrack, headerClick, headerKey, headerMenu, initTracks, renameTo, renameTrack, selectTrack, setVolume, toggleArm, toggleMuteSolo, updateArmed } from "./ui/app/tracks.ts";
import { bounceSong, dirtyUpd, exportSong, importFile, initFiles, markClean, newSong, openSong, save } from "./ui/app/files.ts";
import { attachRoll, centreRoll, detachRoll, drawRoll, fitRoll, followRoll, genRoll, initRoll, inRoll, keyLight, moveRollPh, rollLoopChanged, zoomRoll } from "./ui/app/roll.ts";
import { audible, ensureAudio, initEngine, livePush, onBuffer, onOpenFilters, pushSong, seek, setPlay, songOf, stopPressed } from "./ui/app/engine.ts";
import * as liveState from "./ui/state/live.ts";
import * as tr from "./ui/state/transport.ts";
import { mountTransport } from "./ui/transport/TransportBar.tsx";
import * as files from "./ui/state/files.ts";
import * as lib from "./ui/state/library.ts";
import { mountFiles } from "./ui/files/Files.tsx";
import * as dialogs from "./ui/files/dialogs.tsx";
import { mountLibrary } from "./ui/library/Library.tsx";
import * as arrange from "./ui/state/arrange.ts";
import { mountTracks } from "./ui/arrange/Tracks.tsx";
import { mountRoll } from "./ui/roll/Roll.tsx";
import { installKeys } from "./ui/keys.ts";
import { mountSplitter } from "./ui/common/Splitter.tsx";
import { mountAudition } from "./ui/footer/Audition.tsx";
import * as au from "./ui/state/audition.ts";
import { persist as store } from "./ui/state/persist.ts";  // not `live`: that is the aria-live status element below
import { laneEl, overrides, overrideSet, rollClips, selectedId, setRollShown, setSelectedId } from "./ui/state/view.ts";
import { mountMixer, setMixerShown } from "./ui/mixer.tsx";
// DAW UI (change add-synth-daw): the reviewed mockup's views and interactions on the real song/2 model,
import * as devst from "./ui/state/device.ts";
import * as live1 from "./ui/device/live.ts";
import { autoParams, devName, effectChoices, initParams, oscType, paramOf, slotUsed } from "./ui/model/params.ts";
import { cleanDev, devicesOf, engineDev, fxOf, instOf, laneText, patOf } from "./ui/model/track.ts";
// DAW UI (change add-synth-daw): the reviewed mockup's views and interactions on the real song/1 model,
// engine (AudioWorklet + synth-wasm live host), device panels generated from the engine's parameter tables and
// /api/music. Started by main.ts with { engine, model, api, tables, initial, files, library }.
export function start(DAW) {
let CAT = null;  // library catalog (/api/library)
initSession(DAW);  // engine, api, song store proxies, selection (ui/app/session.ts)
const TABLES = DAW.tables;
const st0 = DAW.initial;
setBars(st0.bars); LOOP[1] = Math.min(BARS, 4) + 1;
// ---- device tables (design D8): every device's parameters come from the engine module (ui/model/params.ts) ----
initParams(TABLES, (k, m, i, p) => E.physical(k, m, i, p));
const devLabel = t => t.bus ? `Bus · ← ${senders(t).map(x => x.name).join(", ") || "no sends"}${fxOf(t).length ? " · " + fxOf(t).map(devName).join(" → ") : ""}`
  : t.devices.map(d => devName(d) + (d._edited ? "*" : "") + (d.bypass ? " (off)" : "")).join(" → ") || "no devices";
let seed = 7; const rnd = () => (seed = (seed * 16807) % 2147483647) / 2147483647;
tr.setTempo(st0.tempo); tr.ensureSig(st0.sig.join("/"));
// Transport bar (Solid, ui/transport/TransportBar.tsx): mounted first, the rest of start() finds its ids. Actions are
// closures: they reach functions defined further down only when clicked.
mountTransport(document.querySelector("header"), {
  play: () => setPlay(!playing), stop: () => stopPressed(), toggleRec: () => setRec(!recOn), toggleLoop: () => toggleLoop(),
  toggleFollow: () => toggleFollow(), zoomOut: () => zoomArr(1 / 1.5), zoomIn: () => zoomArr(1.5), zoomFit: () => fitArr(),
  tempo: v => onTempo(v), sig: v => onSig(v), snap: v => tr.setSnap(v), loopText: str => onLoopText(str),
  length: v => onLength(v), buffer: v => onBuffer(v), openFilters: on => onOpenFilters(on), undo: () => undo(), redo: () => redo(),
  save: () => save(), bounce: () => bounceSong(), exportSong: () => exportSong(),
});
// Songs section and instrument library (Solid, ui/files/Files.tsx, ui/library/Library.tsx)
mountFiles(document.querySelector("#browser > section"), {
  open: f => openSong(f), importFile: f => importFile(f), newSong: () => newSong(), trackCount: () => tracks.length,
});
mountLibrary(document.getElementById("lib"), {
  hasInst: () => { selectedId(); return !!instOf(selected); }, target: () => { selectedId(); return selected.name; },
  audition: (src, it, done) => auditionEntry(src, it, done), load: (src, it) => loadInto(selected, src, it),
});

// Piano roll (ui/app/roll.ts): its audition keys play the selected track.
initRoll({ play: (n, key) => noteOn(selected, n, au.kbVel(), key), stop: key => noteOff(key) });
// Arrangement view (ui/app/arrange.ts): ruler, loop, playheads, follow, zoom; the roll's parts are hooks.
initArrange({
  rollLoop: rollLoopChanged, rollFollow: followRoll, rollPos: moveRollPh,
});
// Clips: ui/arrange/Clip.tsx (reactive; canvas thumbnail redrawn when arrange.bumpLane bumps the lane version).
// Every note occurrence of a placement: [note, step in the clip, audible length]. A looping clip repeats its
// pattern (pattern length = pt.len bars); a cropped clip plays the pattern's start.
function* clipNotes(t, c) {
  const pt = patOf(t, c.name), steps = c.len * 16, per = c.loop ? pt.len * 16 : Infinity;
  for (let k = 0; k === 0 || (c.loop && k * per < steps); k++) for (const n of raw(pt.notes)) {
    if (c.loop && n.s >= per) continue; const s0 = n.s + k * (c.loop ? per : 0); if (s0 < steps) yield [n, s0, Math.min(n.l, steps - s0)];
  }
}
// The lane's content changed (notes, patterns): its clip thumbnails redraw. Position, size and state are reactive.
function renderLane(t) { arrange.bumpLane(t.id); }
function laneValue(pts, bar) {
  pts = raw(pts);  // read-only, runs per lane per frame
  if (bar <= pts[0][0]) return pts[0][1];
  for (let i = 1; i < pts.length; i++) if (bar <= pts[i][0]) { const [b0, v0] = pts[i - 1], [b1, v1] = pts[i]; return v0 + (v1 - v0) * (bar - b0) / (b1 - b0); }
  return pts[pts.length - 1][1];
}
// Automated knobs show their lane's value at the playhead (ui/state/device.ts, view state, not saved). An automated
// knob turned by hand is "overridden" and stops following its lane until re-enabled (Ableton's rule).
let autoSeen = new Map();  // "track:dev:param" → the target written by the last pass (cleared once its lane is gone)
function paramTarget(t, ln) { const d = t.devices[ln.dev]; return d && paramOf(d, ln.param) ? { store: d.values, field: ln.param } : null; }
function applyAutomation() {
  const seen = new Map();
  for (const t of tracks) for (const ln of t.lanes || []) {
    const tg = paramTarget(t, ln); if (!tg) continue;
    const key = ln.dev + ":" + ln.param, ovr = !!overrides.get(t.id)?.has(key);
    // a store write only when the value or override state changed (this runs every frame while playing)
    devst.setAuto(tg.store, tg.field, ovr ? undefined : laneValue(ln.pts, SONG_POS), ovr); seen.set(t.id + ":" + key, tg);
  }
  for (const [k, tg] of autoSeen) if (!seen.has(k)) devst.setAuto(tg.store, tg.field, undefined, false);
  autoSeen = seen;
  for (const l of laneVals) l.ah.classList.toggle("ovr", !!overrides.get(l.t.id)?.has(l.ln.dev + ":" + l.ln.param));
}
function updateReauto(t = selected) {
  const ctx = document.getElementById("ctx"); ctx.querySelector("#reauto")?.remove(); if (!overrides.get(t.id)?.size) return;
  const b = Object.assign(document.createElement("button"), { id: "reauto", textContent: `↺ Re-enable automation (${overrides.get(t.id).size})`, title: "Knobs you turned by hand stopped following their lanes; click to follow them again" });
  b.onclick = () => { overrides.get(t.id).clear(); applyAutomation(); updateReauto(t); live.textContent = `${t.name}: automation re-enabled`; }; ctx.append(b);
}
// ---- Automation record (●, Shift+R; design D8f). While playing, a device knob you turn or hold writes its lane at
// the playhead on a 1/64-bar grid, creating the lane if needed. Touch overwrite: points between this gesture's last
// write and the playhead are replaced, so holding a knob still flattens the old envelope as the song passes. The
// knob keeps following its lane (no override); the engine gets the lane at most every 200 ms; the knob's gesture is
// one undo step. A seek or loop wrap starts a new pass (a write behind the last one never deletes backwards). ----
let recOn = false, recHold = null, recPushAt = 0;
const recLast = new Map();  // gesture key → bar of its last write
function setRec(on) {
  recOn = on; tr.setRec(on); recLast.clear(); document.body.classList.toggle("recording", on);
  say(on ? "automation record on: knobs turned while playing write their lanes" : "automation record off");
}
function recordPoint(t, d, p, v) {
  const di = t.devices.indexOf(d);
  if (di < 0 || p.structural || !autoParams(d).some(x => x.name === p.name)) return false;
  const key = `${t.id}:${di}:${p.name}`, b = 1 + Math.round((SONG_POS - 1) * 64) / 64;
  let ln = t.lanes.find(x => x.dev === di && x.param === p.name), fresh = false;
  // a new lane starts with its point: the push mounts the lane at once (store), which draws it
  if (!ln) { ln = { dev: di, param: p.name, pts: [[b, v]] }; t.lanes.push(ln); t.open = true; fresh = true; }
  const last = recLast.get(key), from = last !== undefined && last <= b ? last : b;
  ln.pts = ln.pts.filter(([x]) => x <= from && x !== b || x > b); ln.pts.push([b, v]); ln.pts.sort((a, c) => a[0] - c[0]);
  if (recHold?.d === d && recHold.p === p) recLast.set(key, b); else recLast.delete(key);  // a pass lasts while the knob is held
  overrides.get(t.id)?.delete(`${di}:${p.name}`);
  if (fresh) rebuildTracks(); else { const l = laneVals.find(x => x.ln === ln); l?.draw(); l?.val(); }
  const now = performance.now(); if (now - recPushAt > 200) { recPushAt = now; pushSong(); } else livePush();
  return true;
}
// A hand edit: overrides automation on that knob, marks the preset edited (*).
function knobEdited(t, d, p) {
  const di = t.devices.indexOf(d), key = `${di}:${p.name}`;
  if (t.lanes.some(l => l.dev === di && l.param === p.name)) { overrideSet(t.id).add(key); applyAutomation(); updateReauto(t); }
  if (d.kind === 64 && !d._edited) { d._edited = true; arrange.setLabelRev(n => n + 1); }  // the panel and the header show the * on the edited preset
}
/** One line from a song/2 about: type · style · tempo · engine · like … — description. */
// Automation: values at the playhead, recording, lane rows (ui/app/automation.ts).
initAutomation({ rebuildTracks: () => rebuildTracks() });
/** One line from a song/1 about: type · style · tempo · engine · like … — description. */
const aboutText = a => [a.type, a.style, a.tempo, a.engine, a.similar_to?.length ? "like " + a.similar_to.join(", ") : ""].filter(Boolean).join(" · ") + " — " + a.description;
const live = document.getElementById("live");
// Track rows: ui/arrange/Tracks.tsx (windowed, design D6). A mounted row in the view window renders its clips and, while
// open, its automation lanes (addLane per lane); rebuildTracks() bumps arrange.rev so mounted rows refill after
// add/delete/reorder/undo. Rows off screen hold nothing (laneEl / laneVals only list mounted rows). Clips render
// reactively in the lane (Clip.tsx), culled to the bars around the view.
arrange.setArmedId(armed?.id ?? null); setSelectedId(selected.id); arrange.setBw(BW);  // the rows mark and pin them from the start
mountTracks(hdrs, lanes, lw, tracks, {
  devLabel: t => devLabel(t), about: t => t.about ? aboutText(t.about) : "", rename: (t, v) => renameTo(t, v),
  pattern: (t, name) => raw(t).pats?.[name] ?? null, autoLane: (t, ln) => addLane(t, ln), addAutoLane: t => addAutoLane(t),
  mute: t => toggleMuteSolo(t, false), solo: t => toggleMuteSolo(t, true), arm: t => toggleArm(t), volume: (t, v, done) => setVolume(t, v, done),
  click: (t, e) => headerClick(t, e), key: (t, e, h) => headerKey(t, e, h), menu: (t, e) => headerMenu(t, e), startRename: t => renameTrack(t),
});


// Knob factory: SVG arc, role=slider, keyboard + drag.
// The value a control shows now (the knob's, which follows automation while playing), else the stored one.
const curVal = (d, name) => { const k = knobReg.get(d.values)?.get(name); return k?.isConnected ? k._value() : vget(d, ip(name)); };
function group(title, knobs, opts = {}) {
  const g = document.createElement("div"); g.className = "grp";
  g.innerHTML = `<div class="gh"><span>${title}</span>${opts.select || ""}</div>`;
  if (opts.onSelect) g.querySelectorAll(".gh select").forEach(se => se.addEventListener("change", () => { const before = snap(); opts.onSelect(se.selectedIndex, se); commit(before, `${se.getAttribute("aria-label")}: ${se.value}`); }));
  if (opts.pre) g.append(opts.pre);
  const k = document.createElement("div"); k.className = "knobs"; k.style.setProperty("--cols", opts.cols || 4);
  for (const a of knobs) k.append(knob(a[0], a[1], a[2], { ...(a[3] || {}), store: opts.store, field: opts.fields?.[a[0]], onChange: opts.onChange, onSet: opts.onSet })); g.append(k); return g;
}
const sel = (label, opts, i = 0) => `<select aria-label="${label}">${opts.map((o, j) => `<option${j === i ? " selected" : ""}>${o}</option>`).join("")}</select>`;
const col = (...els) => { const d = document.createElement("div"); d.className = "dcol"; els.forEach(e => d.append(e)); return d; };
// ---- Device panels generated from the engine's parameter tables (design D8). Values live in the song/2 device
const curVal = (d, name) => devst.autoValue(d.values, name) ?? vget(d, ip(name));  // store reads: screens redraw from them
// ---- Device panels generated from the engine's parameter tables (design D8). Values live in the song/1 device
// (`d.values`, by parameter name); a knob or select writes there, sets the live parameter at once and queues an
// engine update (structural parameters rebuild the device there). ----
const status = document.createElement("div"); status.className = "rack-status"; status.setAttribute("aria-live", "polite");
const trackIndex = t => t.master ? tracks.length : tracks.indexOf(t);
function liveParam(t, d, p, v) {
  if (p.structural) return livePush();
  const di = engineDev(t, d); if (di >= 0 && E.node) E.setParam(trackIndex(t), di, p.index, v);
  livePush();
}
const initInstrument = () => ({ kind: 64, preset: "init", values: { "osc1.type": 0.16666667, "filt1.type": 0.1, "filt1.cutoff": 0.6, "env1.sustain": 0.7, "env1.release": 0.15 } });
// A control for one parameter: select (choices, mod targets), number field (raw sampler index) or knob.
function pctl(t, d, p, label) {
  const short = label ?? p.name.slice(p.name.indexOf(".") + 1).replace(/_/g, " ");
  const set = (v, msg) => { const before = snap(); d.values[p.name] = v; liveParam(t, d, p, v); if (p.structural || p.name.endsWith(".type") || p.name === "mode") renderDevice(); commit(before, msg); };
  if (p.law === "Index" || p.options.length) {
    const s1 = document.createElement("select"); s1.setAttribute("aria-label", `${devName(d)} ${p.name}`);
    const opts = p.law === "Index" ? p.options : p.options, cur = p.law === "Index" ? Math.round(vget(d, p) * 255) : choice(vget(d, p), p.options.length);
    s1.innerHTML = opts.map((o, i) => `<option value="${i}"${i === cur ? " selected" : ""}>${o}</option>`).join("");
    s1.onchange = () => { const i = +s1.value, v = p.law === "Index" ? i / 255 : opts.length > 1 ? i / (opts.length - 1) : 0; set(v, `${t.name}: ${p.name} = ${opts[i]}`); redrawGroup(s1); };
    const w = document.createElement("label"); w.className = "pchoice"; w.innerHTML = `<span>${short}</span>`; w.append(s1); return w;
  }
  if (p.raw) {
    const w = document.createElement("label"); w.className = "pchoice"; w.innerHTML = `<span>${short}</span>`;
    const n = Object.assign(document.createElement("input"), { type: "number", min: 0, max: 65535, step: 1, value: Math.round(vget(d, p)) }); n.className = "num";
    n.setAttribute("aria-label", `${devName(d)} ${p.name}`);
    n.onchange = () => set(Math.max(0, Math.min(65535, Math.round(+n.value || 0))), `${t.name}: ${p.name} = ${n.value}`);
    w.append(n); return w;
  }
  const k = knob(short, vget(d, p), v => fmtVal(d, p, v), { store: d.values, field: p.name, def: p.default,
    onChange: (v, el) => { el._dev = d; if (!(recOn && playing && recordPoint(t, d, p, v))) knobEdited(t, el); liveParam(t, d, p, v); }, onSet: () => {},
    onGrab: el => { if (recOn) recHold = { t, d, p, el }; }, onRelease: () => { if (recHold) recLast.delete(`${t.id}:${t.devices.indexOf(d)}:${p.name}`); recHold = null; } });
  k._dev = d; k.title = p.name; k.dataset.param = p.name; return k;
}
// A group panel: title, choice controls in the header row, knobs below.
function pgroup(t, d, title, ps, opts = {}) {
  const g = document.createElement("div"); g.className = "grp"; g.setAttribute("role", "group"); g.setAttribute("aria-label", title);
  const gh = document.createElement("div"); gh.className = "gh"; gh.innerHTML = `<span>${title}</span>`; g.append(gh);
  if (opts.pre) g.append(opts.pre);
  const k = document.createElement("div"); k.className = "knobs"; k.style.setProperty("--cols", opts.cols || 4);
  for (const p of ps) { if (!p) continue; const c = pctl(t, d, p, opts.labels?.[p.name]); (c.tagName === "LABEL" ? gh : k).append(c); }
  if (k.childElementCount) g.append(k);
  if (opts.pre?._draw) { g._redraw = opts.pre._draw; requestAnimationFrame(() => g.isConnected && g._redraw()); }
  return g;
}
const OSC_SHAPES = { 1: ["waveform"], 2: ["waveform", "pulse width"], 3: ["bandwidth", "harmonics", "damp", "scale", "mod", "drive", "layers", "layer detune", "layer spread"],
  4: ["start", null, null, "loop start", "loop length"],
  // the naive generators (5–11): the colour; type, combine and law are their own parameters
  5: [null, "colour"], 6: [null, "colour"], 7: [null, "colour"], 8: [null, "colour"], 9: [null, "colour"], 10: [null, "colour"], 11: [null, "colour"],
  12: ["wave", "voices", "version", "clock", "retrigger", "filter base", "filter amount", "filter decay", "filter damp"] };  // s1 loop mode, s2 boundary, s5 reverse, s6 interp, s7 source: samplerPre; index: osc<i>.sample
// Sampler shape knobs shown for the current settings: the loop start/length only for a custom loop that plays.
const smpShapes = (d, i) => [0, ...(choice(vget(d, ip(`osc${i}.s1`)), 3) && choice(vget(d, ip(`osc${i}.s2`)), 2) ? [3, 4] : [])];
const slotKeys = (g, i) => INST().params.filter(p => p.name.startsWith(`${g}${i}.`)).map(p => p.name);
/** A choice parameter's option name. */
function optName(d, name) { const p = ip(name), o = p.options || []; return o[choice(vget(d, p), o.length)]; }
/** The stored value of a choice parameter's option `label` (from the engine's table, so no encoding is assumed here). */
function optValue(name, label) { const o = ip(name).options || [], i = o.indexOf(label); return i <= 0 || o.length < 2 ? 0 : i / (o.length - 1); }
// oscillator kind ids (unisynth `osc_type`): 5–11 the naive generators, 12 clocked; a literal, since oscType is hoisted
// (fmtVal may run before this line)
function oscType(d, i) {
  return Math.max(0, ["Off", "Phase mod", "Saw/pulse", "Wavetable", "Sampler", "Naive saw/tri", "Naive pulse", "Sine", "Coloured noise", "FM",
    "Aux in A", "Aux in B", "Clocked"].indexOf(optName(d, `osc${i}.type`)));
}
function slotUsed(d, g, i) {
  if (g === "osc") return oscType(d, i) !== 0;
  if (g === "filt") return optName(d, `filt${i}.type`) !== "Off";
  if (g === "mod") return modSrcName(d, i) !== "None";
  if (g === "env" && i === 1) return true;
  return slotKeys(g, i).some(n => n in d.values && d.values[n] !== ip(n).default) || (g === "env" || g === "lfo") && routesOf(d, g, i).length > 0;
}
/** A choice parameter's option name: expanded choices (`Law::Kinds`) store later kinds between the old ones. */
// Slot edits rewrite mod-matrix targets and automation lanes in one command (design D8a). `map(slot)` → new slot
// number or 0 (gone) for every slot of group g.
function remapSlots(t, d, g, map) {
  const n = SLOTS[g], old = {};
  for (let i = 1; i <= n; i++) for (const k of slotKeys(g, i)) if (k in d.values) { (old[i] ??= {})[k.slice(k.indexOf("."))] = d.values[k]; delete d.values[k]; }
  for (let i = 1; i <= n; i++) { const j = map(i); if (j && old[i]) for (const [f, v] of Object.entries(old[i])) d.values[`${g}${j}${f}`] = v; }
  // a group's freed slots switch off when their type default is on
  for (let j = 1; j <= n; j++) { const ty = ip(`${g}${j}.type`); if (ty && ty.default !== 0 && !(`${g}${j}.type` in d.values) && ![...Array(n).keys()].some(i => map(i + 1) === j && old[i + 1])) d.values[`${g}${j}.type`] = 0; }
  // mod targets: osc2.gain → osc1.gain …; a target in a removed slot clears its mod slot
  const names = TARGETS(), re = new RegExp(`^${g}(\\d+)(\\..+)$`);
  for (let m = 1; m <= SLOTS.mod; m++) {
    const tk = `mod${m}.target`; if (!(tk in d.values)) continue; const nm = names[Math.round(d.values[tk] * 255)] || "", mm = nm.match(re); if (!mm) continue;
    const j = map(+mm[1]); if (!j) { d.values[`mod${m}.source`] = 0; continue; }
    const id = names.indexOf(`${g}${j}${mm[2]}`); if (id >= 0) d.values[tk] = id / 255;
  }
  if (g === "mod") { /* mod slots themselves are not mod targets of each other here beyond modN.amount */ }
  const di = t.devices.indexOf(d), lre = new RegExp(`^${g}(\\d+)(\\..+)$`);
  t.lanes = t.lanes.filter(ln => { if (ln.dev !== di) return true; const mm = ln.param.match(lre); if (!mm) return true; const j = map(+mm[1]); if (!j) return false; ln.param = `${g}${j}${mm[2]}`; return true; });
}
// Fixed slot blocks (change add-device-live-view D1): every slot at its own position; a slot not set starts collapsed
// to its header (type control); ✕ clears in place; only filters swap (serial order). Slot collapse overrides live
// for the session (per track), block collapse in `store` (editor state, never in the song).
// a mod row's source name ("None", "Env 1", "LFO 2", "Velocity", V2 CCs …) from the parameter table
function slotCmd(t, msg, fn) { const before = snap(); fn(); rebuildTracks(); renderDevice(); status.textContent = msg; commit(before, msg); }
// Slot screens (D4): an SVG in a 100×40 box (stretched), redrawn from the current values through the group's
// `_redraw` (knobs, automation, selects). Drawings, not measurements: shapes follow the parameters' meaning.
const SVGNS = "http://www.w3.org/2000/svg";
function scrSvg(draw) {
  const svg = document.createElementNS(SVGNS, "svg"); svg.setAttribute("class", "scr"); svg.setAttribute("viewBox", "0 0 100 40");
  svg.setAttribute("preserveAspectRatio", "none"); svg.setAttribute("aria-hidden", "true");
  svg._draw = () => { svg.innerHTML = draw(); }; return svg;
}
const scrPath = (f, n = 160) => "M" + Array.from({ length: n + 1 }, (_, k) => `${(k * 100 / n).toFixed(2)} ${Math.max(.5, Math.min(39.5, f(k / n))).toFixed(2)}`).join(" L");
const scrLine = (d, fill) => (fill ? `<path class="fl" d="${d} L100 40 L0 40 Z"/>` : "") + `<path class="ln" d="${d}"/>`;
const scrMid = `<line class="mk0" x1="0" y1="20" x2="100" y2="20"/>`;
function oscView(d, i) {
  return scrSvg(() => {
    const ty = oscType(d, i), v = n => curVal(d, `osc${i}.${n}`), TAU = 2 * Math.PI;
    let f;
    if (ty === 2) { const pulse = choice(v("s0"), 2), pw = .05 + .9 * v("s1"); f = x => { const ph = (x * 2) % 1; return 20 - 15 * (pulse ? (ph < pw ? 1 : -1) : 2 * ph - 1); }; }
    else if (ty === 3) { const hn = 1 + Math.round(v("s1") * 15), dm = 1 + v("s2") * 3; let pk = 0; const g = x => { let y = 0; for (let k = 1; k <= hn; k++) y += Math.sin(TAU * 2 * k * x) / k ** dm; return y; };
      for (let k = 0; k <= 64; k++) pk = Math.max(pk, Math.abs(g(k / 128))); f = x => 20 - 15 * g(x) / (pk || 1); }
    else { const fb = v("feedback") * 2.5; f = x => 20 - 15 * Math.sin(TAU * 2 * x + fb * Math.sin(TAU * 2 * x)); }
    return scrMid + scrLine(scrPath(f));
  });
}
// filter response on a 20 Hz…20 kHz log axis (x = 100·log10(f/20)/3), 0 dB near the top, −60 dB at the bottom
const yDb = db => 38 - Math.max(0, Math.min(36, (db + 60) * .55));
function filtView(t, d, i) {
  return scrSvg(() => {
    const p = ip(`filt${i}.cutoff`), hz = E.physical(d.kind, 0, p.index, curVal(d, p.name)), cx = Math.max(0, Math.min(100, Math.log10(hz / 20) / .03));
    const name = (p0 => p0.options[choice(vget(d, p0), p0.options.length)] || "")(ip(`filt${i}.type`)).toLowerCase();
    const q = .6 + curVal(d, `filt${i}.resonance`) * 6, poles = /ladder/.test(name) ? 2 : 1;
    const h = x => { let r = 10 ** ((x - cx) * .03); if (/high/.test(name)) r = 1 / r;
      const den = Math.sqrt((1 - r * r) ** 2 + (r / q) ** 2), lp = 1 / den;
      return /band/.test(name) ? (r / q) / den : /notch/.test(name) ? Math.abs(1 - r * r) / den : lp ** poles; };
    return scrLine(scrPath(x => yDb(20 * Math.log10(h(x * 100) + 1e-9))), true) + `<line class="mk" x1="${cx.toFixed(2)}" y1="0" x2="${cx.toFixed(2)}" y2="40"/>`;
  });
}
const LFO_SHAPES = [x => Math.sin(2 * Math.PI * x), x => 2 * x - 1, x => 1 - 2 * x, x => x < .5 ? 1 : -1, x => Math.sin(Math.floor(x * 8) * 2.7 + 1)];
function lfoView(d, i) {
  return scrSvg(() => { const f = LFO_SHAPES[choice(curVal(d, `lfo${i}.shape`), 5)], dep = .25 + .75 * curVal(d, `lfo${i}.depth`);
    return scrMid + scrLine(scrPath(x => 20 - 15 * dep * f(x), 200)); });
}
// Envelope curve as the engine plays it (crates/unisynth/src/env.rs): WaveSabre mode in closed form (linear attack,
// quadratic decay and release, times `p²·5000 + 1` ms), Tunefish mode integrated per 512-frame block. A note is held
// for a while after the decay, then released. Redrawn from the knobs (`_draw`), so it follows edits and automation.
// Tunefish-mode stage times in seconds (same integration as envCurve): attack 0 → 1, decay 1 → sustain (with the
// current sustain and slope), release from full level to silence. Infinity when the stage never ends.
function envView(d, i) {
  const W = 196, H = 54, NS = "http://www.w3.org/2000/svg";
  const svg = document.createElementNS(NS, "svg"); svg.setAttribute("class", "env"); svg.setAttribute("viewBox", `0 0 ${W} ${H}`); svg.setAttribute("aria-hidden", "true");
  svg._draw = () => {
    const v = n => curVal(d, `env${i}.${n}`), tf = envRate(v("law"));
    const { pts, off } = envCurve(tf, v("attack"), v("decay"), v("sustain"), v("release"), v("curve_slope"));
    const tot = Math.max(pts[pts.length - 1][0], 1e-3), x = t1 => 4 + (t1 / tot) * (W - 8), y = q => H - 10 - q * (H - 14);
    svg._geo = { x, y, off, tot };  // the live dots use the curve's own time axis
    const step = Math.max(1, Math.ceil(pts.length / 200)), line = pts.filter((_, k) => k % step === 0 || k === pts.length - 1).map(([t1, q]) => x(t1).toFixed(1) + "," + y(q).toFixed(1)).join(" ");
    const fmt = s => s >= 1 ? s.toFixed(2) + " s" : Math.round(s * 1000) + " ms";
    svg.innerHTML = `<line x1="${x(off)}" y1="2" x2="${x(off)}" y2="${H - 10}" stroke="${css("--line")}" stroke-dasharray="2 2"/>
      <polyline fill="currentColor" fill-opacity=".14" stroke="currentColor" stroke-width="1.5" points="${x(0)},${y(0)} ${line} ${x(tot)},${y(0)}"/>
      <text x="${x(off) > W - 40 ? x(off) - 2 : x(off) + 2}" y="9" font-size="8" text-anchor="${x(off) > W - 40 ? "end" : "start"}" fill="${css("--dim")}">note off</text>
      <text x="${W - 4}" y="${H - 1}" font-size="8" text-anchor="end" fill="${css("--dim")}">${fmt(tot)}${tf ? " · rate law" : ""}</text>`;
    svg.closest(".grp")?.querySelectorAll(".knob").forEach(k => k._redrawText?.());
      <text x="${W - 4}" y="${H - 1}" font-size="8" text-anchor="end" fill="${css("--dim")}">${fmt(tot)}${tf ? " · Tunefish" : ""}</text>`;
  };
  return svg;
}
// Sampler slot (unisynth smp_source): `osc<i>.s7` picks the source — choice of 3: GSM sample of the device (0), GM.DLS
// instrument (0.5; a region per note: index = 1 + drums·16384 + bank·128 + program) or one GM.DLS wave (1.0; index =
// wave + 1) — and `osc<i>.sample` holds the raw index (0 = none). Both are structural (the engine rebuilds the device).
const NOBANK = new URLSearchParams(location.search).has("nobank");
/** What a sampler slot plays (ui/device/Sampler.tsx shows it); store and catalog reads, so the line follows them. */
function smpWhat(d, i) {
  const src = choice(vget(d, ip(`osc${i}.s7`)), 3), idx = Math.round(vget(d, ip(`osc${i}.sample`))), nGsm = d.samples?.length ?? 0, gm = lib.catalog()?.gm;
  if (src === 0) { const kb = idx && idx <= nGsm ? ` · ${(d.samples[idx - 1].length * 3 / 4 / 1024).toFixed(1)} KB` : "";
    return idx ? `GSM sample ${idx} of ${nGsm} in this instrument${idx > nGsm ? " (missing: plays silence)" : kb}` : `no GSM sample chosen (${nGsm} in this instrument)`; }
  if (src === 1) { const k = gmInstrument(idx), g = k && gm?.instruments.find(x => !!x.drums === k.drums && x.bank === k.bank && x.prog === k.prog);
    return k ? `GM.DLS ${k.drums ? "drum kit" : "instrument"} ${g ? `“${g.name}”` : ""} bank ${k.bank} program ${k.prog}${g ? ` · ${g.regions.length} regions` : gm ? " (not in this bank)" : ""}` : "no GM instrument chosen"; }
  const g = gm?.instruments.find(x => x.regions.some(r => r[2] === idx - 1)); return idx ? `GM.DLS wave ${idx - 1}${g ? ` · from ${g.name}` : ""}` : "no wave chosen";
}
const pcmCache = new Map();
const pcmOf = b64 => { if (!pcmCache.has(b64)) pcmCache.set(b64, E.gsmDecode(b64ToBytes(b64))); return pcmCache.get(b64); };
// ---- GSM samples: stored (base64) in the instrument's `samples`; a sampler slot plays one by 1-based number. From the
// library (samples in project songs), or imported from an audio file: decoded and resampled to 44.1 kHz by the
// browser, mixed to mono, encoded to GSM 06.10 by the engine (bit-exact libgsm port, about 9 KB per second). ----
const gsmSongs = new Map();  // song path → parsed song (library loads)
async function gsmOf(g) {
  if (!gsmSongs.has(g.path)) gsmSongs.set(g.path, songText(await api.read(g.path)));
  const s = gsmSongs.get(g.path), t = [...s.tracks, s.master].find(x => x.id === g.track);
  return t?.devices?.[g.device]?.samples?.[g.sample] ?? null;
}
let previewCtx = null, previewSrc = null;
async function playSample(b64) {
  const pcm = E.gsmDecode(b64ToBytes(b64)); if (!pcm) { say("not a GSM sample"); return; }
  previewCtx ??= new AudioContext({ sampleRate: 44100 }); await previewCtx.resume();
  try { previewSrc?.stop(); } catch {}
  const buf = previewCtx.createBuffer(1, pcm.length, 44100); buf.copyToChannel(pcm, 0);
  previewSrc = previewCtx.createBufferSource(); previewSrc.buffer = buf; previewSrc.connect(previewCtx.destination); previewSrc.start();
  say(`playing the sample (${(pcm.length / 44100).toFixed(2)} s)`);
}
const gsmSlot = d => [1, 2, 3].find(i => oscType(d, i) === 4 && choice(vget(d, ip(`osc${i}.s7`)), 3) === 0) || [1, 2, 3].find(i => oscType(d, i) === 4) || [1, 2, 3].find(i => !slotUsed(d, "osc", i)) || 1;
function useGsm(t, d, b64, label, slot = gsmSlot(d)) {
  const before = snap(); d.samples ??= [];
  let k = d.samples.indexOf(b64); if (k < 0) { d.samples.push(b64); k = d.samples.length - 1; }
  Object.assign(d.values, { [`osc${slot}.type`]: optValue(`osc${slot}.type`, "Sampler"), [`osc${slot}.s7`]: 0, [`osc${slot}.sample`]: k + 1 });
  overrides.get(t.id)?.clear(); livePush(); rebuildTracks(); renderAll(); commit(before, `${t.name}: osc ${slot} plays ${label} (GSM sample ${k + 1})`);
}
// Samples no GSM sampler slot plays still cost bytes in the intro: drop them and renumber the slots.
function gsmUsed(d) { return new Set([1, 2, 3].filter(i => oscType(d, i) === 4 && choice(vget(d, ip(`osc${i}.s7`)), 3) === 0).map(i => Math.round(vget(d, ip(`osc${i}.sample`))))); }
function removeUnusedGsm(t, d) {
  const used = gsmUsed(d), before = snap(), keep = [], map = new Map();
  (d.samples || []).forEach((b, k) => { if (used.has(k + 1)) { keep.push(b); map.set(k + 1, keep.length); } });
  const n = (d.samples || []).length - keep.length; d.samples = keep;
  for (const i of [1, 2, 3]) if (oscType(d, i) === 4 && choice(vget(d, ip(`osc${i}.s7`)), 3) === 0) d.values[`osc${i}.sample`] = map.get(Math.round(vget(d, ip(`osc${i}.sample`)))) ?? 0;
  livePush(); renderDevice(); commit(before, `${t.name}: removed ${n} unused GSM sample${n === 1 ? "" : "s"}`);
}
const wavIn = Object.assign(document.createElement("input"), { type: "file", accept: "audio/*,.wav,.aif,.aiff,.flac,.mp3,.ogg" });
wavIn.hidden = true; wavIn.setAttribute("aria-label", "Audio file to import as a GSM sample"); document.body.append(wavIn);
let wavTarget = null;
function importWav(t, d, slot) { wavTarget = { t, d, slot }; wavIn.value = ""; wavIn.click(); }
const GSM_MAX_SECONDS = 30;
async function importAudio(file, tg) {
  try {
    const ab = await new OfflineAudioContext(1, 1, 44100).decodeAudioData(await file.arrayBuffer());  // resampled to 44.1 kHz
    const n = ab.length, mono = new Float32Array(n);
    for (let c = 0; c < ab.numberOfChannels; c++) { const ch = ab.getChannelData(c); for (let i = 0; i < n; i++) mono[i] += ch[i] / ab.numberOfChannels; }
    if (n > GSM_MAX_SECONDS * 44100) return say(`${file.name}: ${(n / 44100).toFixed(1)} s is too long for a 64k intro (at most ${GSM_MAX_SECONDS} s); trim it first`);
    let peak = 0; for (const x of mono) peak = Math.max(peak, Math.abs(x));
    const chunk = E.gsmEncode(mono);
    useGsm(tg.t, tg.d, bytesToB64(chunk), file.name, tg.slot);
    say(`${file.name}: ${(n / 44100).toFixed(2)} s → ${(chunk.length / 1024).toFixed(1)} KB GSM${peak > 1 ? " · clipped: the file peaks above 0 dBFS" : ""}`);
  } catch (e) { say(`${file.name}: cannot read this audio file (${e?.message || e})`); }
}
wavIn.onchange = () => { const f = wavIn.files?.[0]; if (f && wavTarget) importAudio(f, wavTarget); };
// Effect chain edits keep automation lanes (device index) in step with t.devices.
function devBypass(t, i) { const d = t.devices[i], before = snap(); d.bypass = !d.bypass; renderAll(); rebuildTracks(); commit(before, `${t.name}: ${devName(d)} ${d.bypass ? "bypassed" : "on"}`); }
function moveDev(t, i, dir) {
  let j = i + dir; while (j >= 0 && j < t.devices.length && t.devices[j].kind === 64) j += dir;  // the instrument stays first
  if (j < 0 || j >= t.devices.length) return; const before = snap(), name = devName(t.devices[i]);
  [t.devices[i], t.devices[j]] = [t.devices[j], t.devices[i]];
  for (const ln of t.lanes) ln.dev = ln.dev === i ? j : ln.dev === j ? i : ln.dev;
  rebuildTracks(); renderAll(); commit(before, `${t.name}: ${name} moved ${dir < 0 ? "earlier" : "later"} (its automation lanes follow)`);
}
function removeDev(t, i) {
  const before = snap(), name = devName(t.devices[i]), gone = t.lanes.filter(l => l.dev === i).length;
  t.devices.splice(i, 1); t.lanes = t.lanes.filter(l => l.dev !== i); t.lanes.forEach(l => { if (l.dev > i) l.dev--; });
  rebuildTracks(); renderAll(); commit(before, `${t.name}: ${name} removed${gone ? `, with ${gone} automation lane${gone > 1 ? "s" : ""}` : ""}`);
}
function addEffect(t, label) {
  const tb = effectChoices().find(x => x.label === label); if (!tb) return; const before = snap(), n = nModes(tb.kind);
  t.devices.push({ kind: tb.kind, values: { mode: n > 1 ? tb.mode / (n - 1) : 0 } }); rebuildTracks(); renderAll(); commit(before, `${t.name}: ${label} added`);
}
function savePreset(t, d, btn) {
  const pop = document.createElement("div"); pop.className = "menu"; pop.setAttribute("role", "dialog"); pop.setAttribute("aria-label", "Save preset");
  pop.innerHTML = `<label class="dim" style="padding:4px 6px">Preset name<input class="pname" value="${(d.preset || "my patch").replace(/^GM /, "")}" style="width:100%"></label>
    <div class="dim small" style="padding:0 6px 4px">→ project/music/patches/<b class="pfile"></b></div><div style="display:flex;gap:4px;padding:4px 6px"><button class="ok">Save</button><button class="no">Cancel</button></div><div class="perr dim small" role="alert"></div>`;
  document.body.append(pop); const r = btn.getBoundingClientRect(); pop.style.left = Math.min(r.left, innerWidth - 260) + "px"; pop.style.top = r.bottom + 4 + "px";
  const inp = pop.querySelector(".pname"), file = () => inp.value.trim().toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-|-$/g, "") + ".patch.json";
  const upd = () => { pop.querySelector(".pfile").textContent = file(); pop.querySelector(".ok").disabled = !inp.value.trim(); }; upd(); inp.oninput = upd; inp.select(); inp.focus();
  const close = () => { pop.remove(); btn.focus(); };
  pop.querySelector(".no").onclick = close;
  pop.querySelector(".ok").onclick = async () => {
    const nm = inp.value.trim(), path = `project/music/patches/${file()}`;
    const { _edited, ...dev } = d; const text = E.presetSave({ ...dev, values: { ...d.values } }, nm);
    const res = await api.write(path, text);
    if (!res.ok) { pop.querySelector(".perr").textContent = res.errors?.map(e => `${e.path}: ${e.message}`).join("; ") || res.error; return; }
    const before = snap(); d.preset = nm; d._edited = false; close(); CAT = await api.library() || CAT; lib.setCatalog(CAT); renderAll(); btn.focus(); commit(before, `saved preset ${nm} → ${path}`);
  };
  pop.addEventListener("keydown", e => { if (e.key === "Escape") close(); if (e.key === "Enter") pop.querySelector(".ok").click(); });
}
const device = document.getElementById("device");
live1.initLive({ on: liveOn, inst: () => instOf(selected) ?? null, curVal, choice, lfoShape: k => LFO_SHAPES[k], yDb, noteName: n => NAMES[n], rate: () => E.rate });
let devMounted = false;
function renderDevice() {
  const t = selected;
  if (!devMounted) { devMounted = true; mountDevice(device, deviceActions()); }
  devst.setShownTrack(t);
  const clip = t.clips?.find(c => c.sel) || t.clips?.[0];
  document.getElementById("ctx").innerHTML = `<span class="dim">Track</span> <b style="color:var(${t.col})">${t.name}</b>` +
    (clip ? ` <span class="dim">· clip</span> <b>${clip.name}</b>${links(t, clip.name) > 1 ? ` <span class="badge">⧉ ${links(t, clip.name)} linked</span>` : ""}` : "") +
    ` <span class="dim">· ${t.devices.length} device${t.devices.length === 1 ? "" : "s"}</span>`;
  applyAutomation(); updateReauto(t);
  updateWatch(); live1.markLiveDirty();
}
const initInstrument = () => ({ kind: 64, preset: "init", values: { "osc1.type": 0.5, "filt1.type": 0.125, "filt1.cutoff": 0.6, "env1.sustain": 0.7, "env1.release": 0.15 } });
// Device panel, commands and live view (ui/app/device.ts).
initDevice();

// Track commands and song files (ui/app/tracks.ts, ui/app/files.ts).
initTracks(); initFiles();

// Mixer: each strip = inserts (the track's effect chain), sends (stored as receives on the target track,
// post-fader), fader, meters. Bus tracks have no instrument; the master is the last track and can hold effects.
const mixer = document.getElementById("mixer");
const FX = () => effectChoices().map(tb => tb.label);
const trackById = id => id === master.id ? master : tracks.find(x => x.id === id);
let sendBefore = null;  // undo snapshot of a send-level gesture
mountMixer(mixer, song, {
  select: id => selectTrack(trackById(id)),
  mute: id => toggleMuteSolo(trackById(id), false), solo: id => toggleMuteSolo(trackById(id), true),
  volume: (id, v, done) => setVolume(trackById(id), v, done),
  openInsert: id => { selectTrack(trackById(id)); show(tabs[1]); },
  bypass: (id, i) => devBypass(trackById(id), i),
  effects: FX,
  addInsert: (id, label) => addEffect(trackById(id), label),
  // targets: any track except itself, the master, and tracks that already feed this one (would loop)
  sendTargets: id => {
    const t = trackById(id), feeds = new Set([t.name]); let grew = true;
    while (grew) { grew = false; for (const x of tracks) if (!feeds.has(x.name) && x.sends.some(s1 => feeds.has(s1[0]))) { feeds.add(x.name); grew = true; } }
    return tracks.filter(x => !feeds.has(x.name)).flatMap(x => [[x.name, "main"], [x.name, "side"]]);
  },
  addSend: (id, to, ch) => { const t = trackById(id), before = snap(); t.sends.push([to, ch, -12]); renderAll(); commit(before, `send ${t.name} → ${to} (${ch === "side" ? "sidechain" : "main"}, post-fader)`); },
  removeSend: (id, i) => { const t = trackById(id), before = snap(), [to] = t.sends[i]; t.sends.splice(i, 1); renderAll(); commit(before, `send ${t.name} → ${to} removed`); },
  sendLevel: (id, i, db, done) => {
    const t = trackById(id), sd = t.sends[i];
    if (done) { if (sendBefore) commit(sendBefore, `send ${t.name} level ${db.toFixed(1)} dB`); sendBefore = null; return; }
    sendBefore ??= snap(); sd[2] = db; sd[3] = undefined; livePush();
  },
  addBus: () => addTrack(true),
  devName,
});



// ===== Editing: selection, cut/copy/paste/duplicate/delete, move/transpose, quantize/align, undo =====
// Status, undo/redo (ui/app/core.ts): a restore rebuilds the views for the restored song and selection.
initCore({
  restored: () => { rebuildTracks(); selectTrack(selected); updateArmed(); },
  changed: () => { dirtyUpd(); livePush(); },
});


// Clip editing on the lanes (ui/app/clips.ts).
initClips({
  fitSong: quiet => fitSong(quiet), renderDevice: () => renderDevice(), selectTrack: t => selectTrack(t),
  clipPicked: (t, c) => { rollClips.set(t.id, c); selectTrack(t); centreRoll(); },
  showRoll: () => { show(tabs.find(x => x.id === "tab-roll")); centreRoll(); },
  rollChanged: redraw => { genRoll(selected); if (redraw) drawRoll(); },
});

async function loadInto(t, src, it) {
  const before = snap(); let inst = instOf(t); if (!inst) return;
  if (src.id === "project") {
    const text = await api.read(it.preset.path), r = E.presetLoad(text);
    if (!r.device) return say(`${it.name}: ${r.errors?.map(e => e.message).join("; ")}`);
    const i = t.devices.indexOf(inst);
    const lost = t.lanes.filter(ln => ln.dev === i && !paramOf(r.device, ln.param)).map(ln => ln.param);
    if (lost.length) return say(`${it.name}: the lanes on ${lost.join(", ")} have no parameter in this patch`);
    t.devices[i] = { ...r.device, _edited: false }; inst = t.devices[i];
  } else if (it.conv?.devices?.length) {
    // converted legacy entry: its instrument replaces the track's, its insert effects go right after it
    const [ins, ...fx] = it.conv.devices.map(d => ({ ...structuredClone(d), preset: it.name, _edited: false }));
    const i = t.devices.indexOf(inst);
    const lost = t.lanes.filter(ln => ln.dev === i && !paramOf(ins, ln.param)).map(ln => ln.param);
    if (lost.length) return say(`${it.name}: the lanes on ${lost.join(", ")} have no parameter in this patch`);
    t.devices.splice(i, 1, ins, ...fx); inst = ins;
    for (const ln of t.lanes) if (ln.dev > i) ln.dev += fx.length;
  } else if (src.id === "gsm") {
    const b64 = await gsmOf(it.gsm); if (!b64) return say(`${it.name}: sample not found in ${it.gsm.path}`);
    return useGsm(t, inst, b64, it.name);
  } else if (src.id === "gm") {
    // a sampler slot on the GM.DLS instrument (source 0.5): each note plays the region for its key and velocity
    const g = it.gm, slot = [1, 2, 3].find(i => oscType(inst, i) === 4) || [1, 2, 3].find(i => !slotUsed(inst, "osc", i)) || 1;
    Object.assign(inst.values, { [`osc${slot}.type`]: optValue(`osc${slot}.type`, "Sampler"), [`osc${slot}.s7`]: 0.5, [`osc${slot}.sample`]: 1 + (g.drums ? 16384 : 0) + g.bank * 128 + g.prog });
    delete inst.values[`osc${slot}.s8`];
    inst.preset = `GM ${it.gm.name}`; inst._edited = false;
  }
  overrides.get(t.id)?.clear(); rebuildTracks(); renderAll(); say(`${t.name}: loaded “${it.name}” (one undo step)`); commit(before, `${t.name}: loaded ${it.name}`);
}
CAT = DAW.library; lib.setCatalog(CAT);
if (CAT?.gm && E.bank == null) api.bank().then(b => { if (b) E.setBank(b); });
genRoll(selected);
renderAll();

// Tabs (roving, arrow keys)
const tabs = [...document.querySelectorAll('[role="tab"]')];
function show(t) {
  if (t.id !== "tab-roll") setRollShown(false);  // unmount while still laid out: detachRoll keeps the scroll position
  for (const x of tabs) { const on = x === t; x.setAttribute("aria-selected", on); x.tabIndex = on ? 0 : -1; document.getElementById(x.getAttribute("aria-controls")).hidden = !on; }
  setRollShown(t.id === "tab-roll");  // the roll panel is mounted only while its tab shows (attachRoll draws it, laid out)
  if (t.id === "tab-roll") { drawRoll(); centreRoll(); }
  setMixerShown(t.id === "tab-mix");  // the mixer is mounted only while its tab shows
  devst.setDeviceShown(t.id === "tab-dev");  // so is the device panel
  updateWatch();
}
tabs.forEach((t, i) => { t.addEventListener("click", () => show(t)); t.addEventListener("keydown", e => {
  if (e.key === "ArrowRight" || e.key === "ArrowLeft") { const n = tabs[(i + (e.key === "ArrowRight" ? 1 : tabs.length - 1)) % tabs.length]; n.focus(); show(n); }
}); });
const initial = new URLSearchParams(location.search).get("tab");
show(tabs.find(t => t.id === "tab-" + initial) || tabs[1]);


// Splitters (ui/common/Splitter.tsx): drag or arrows, sizes remembered in the browser.
mountSplitter(document.getElementById("split-v-slot"), { id: "split-v", label: "Resize browser", orientation: "vertical", cssVar: "--browser-w", min: 140, max: 360,
  measure: () => document.getElementById("browser").getBoundingClientRect().width, at: e => e.clientX });
mountSplitter(document.getElementById("split-h-slot"), { id: "split-h", label: "Resize arrangement", orientation: "horizontal", cssVar: "--arr-h", min: 120, max: 700,
  measure: () => document.getElementById("arr").getBoundingClientRect().height, at: e => e.clientY - document.getElementById("work").getBoundingClientRect().top });

// ===== Transport: song length, loop, seek, playback (clock + meters + a small WebAudio preview voice per track) =====
function onTempo(v) { tr.setTempo(v); showPos(); say(`tempo ${v} BPM`); dirtyUpd(); livePush(); }
function onSig(v) { tr.setSig(v); document.documentElement.style.setProperty("--beats", beatsPerBar()); showPos(); dirtyUpd(); say(`time signature ${v}: beat grid and positions follow; the piano roll keeps 16 steps per bar`); }




// ---- Engine transport (design D6, D8e): the AudioWorklet live host plays the song; the UI clock follows the
// position it reports. Every edit sends the exported song to the host, which applies it per track (D4). ----
const frameOf = b => Math.max(0, Math.round((b - 1) / barsPerSec() * E.rate));
const barOf = f => 1 + f / E.rate * barsPerSec();
let songMeta = st0.meta, songLen0 = st0.length0;  // song/2 meta (title, author, licence notes) and [bars, ticks] as loaded
const uiState = () => ({ tracks: raw(tracks), master: raw(master), tempo: tr.tempo() || 120, sig: sig(), bars: BARS, length0: songLen0, ...(songMeta ? { meta: songMeta } : {}) });
const songOf = live => { const s1 = M.toSong(uiState(), live ? audible : undefined); for (const t of [...s1.tracks, s1.master]) t.devices = t.devices.map(cleanDev); return s1; };
let liveLoaded = false, pushT = 0, pushing = false, statusAt = 0, lastIssue = "";
// One push at a time; a push asked for while one runs runs once more after it (never dropped: a Play or an edit
// during a long load still reaches the engine). Resolves when the engine has the current song.
let pushQ = Promise.resolve(), pushAgain = false;
function pushSong() {
  if (pushing) { pushAgain = true; return pushQ; }
  pushQ = (async () => { do { pushAgain = false; await pushOnce(); } while (pushAgain && E.node); })();
  return pushQ;
}
async function pushOnce() {
  if (!E.node) return; pushing = true;
  try {
    const r = E.exportBlob(JSON.stringify(songOf(true)));
    if (!r.blob) { const e = r.errors?.[0]; lastIssue = e ? `${e.path}: ${e.message}` : "export failed"; say(`engine not updated: ${lastIssue}`); return; }
    lastIssue = "";
    const res = await (liveLoaded ? E.update(r.blob) : E.load(r.blob));
    if (!res.ok) { say(`engine: ${res.reply?.errors?.[0]?.message || "update failed"}`); liveLoaded = false; return; }
    if (!liveLoaded) { liveLoaded = true; liveLoop(); }
    if (playing && liveLoaded) tr.setEdited(true);
  } finally { pushing = false; }
}
function livePush() { if (!E.node) return; clearTimeout(pushT); pushT = setTimeout(pushSong, 30); }
function liveLoop() { if (E.node && liveLoaded) E.setLoop(frameOf(LOOP[0]), frameOf(LOOP[1]), loopOn); }
// Run open filters (listening switch, remembered): live playback only, see Engine.runOpenFilters.
tr.setOpenFilters(store.get("daw.openflt") === "1"); E.runOpen = tr.openFilters();
function onOpenFilters(on) { tr.setOpenFilters(on); store.set("daw.openflt", on ? "1" : "0"); E.runOpenFilters(on); say(`open filters ${on ? "run (as WaveSabre)" : "skipped"} in live playback`); }
// Output buffer (Buffer select, remembered): applied by rebuilding the audio graph, keeping position and play state.
const bufHint = v => (isNaN(+v) ? v : +v);
{ const v = store.get("daw.buffer"); if (v && tr.BUFFERS.some(([x]) => x === v)) tr.setBuffer(v); E.latency = bufHint(tr.buffer()); }
async function onBuffer(v) {
  tr.setBuffer(v); store.set("daw.buffer", v); E.latency = bufHint(v);
  if (!E.node) return;
  const was = playing; if (was) await setPlay(false);
  await E.restart(44100); liveLoaded = false;
  if (!(await ensureAudio())) return say(`cannot restart audio: ${lastIssue || "audio did not start"}`);
  E.seek(frameOf(SONG_POS));
  say(`output buffer ${Math.round(E.bufferSeconds * 1000)} ms`);
  if (was) await setPlay(true);
}
// A dead engine (trap in the worklet) must not leave a frozen readout: stop, zero the meters, say so; Play restarts it.
E.onError = msg => {
  liveLoaded = false; if (playing) setPlay(false); setLoad(0); meters({ peaks: [] });
  say(`audio engine stopped (${msg}); press Play to restart it`);
};
async function ensureAudio() {
  if (!E.node || E.dead) { await E.start(44100); liveLoaded = false; }
  else await E.ctx.resume();
  if (!liveLoaded) await pushSong();
  return liveLoaded;
}
function seek(b1) {
  resumeFollow(); SONG_POS = Math.max(1, Math.min(BARS + PAD_BARS + 1, b1));
  if (E.node) E.seek(frameOf(SONG_POS));
  statusAt = performance.now(); showPos(); applyAutomation(); laneVals.forEach(l => l.val());
}
async function setPlay(on) {
  if (on) {
    if (!(await ensureAudio())) return say(`cannot play: ${lastIssue || "audio did not start"}`);
    liveLoop(); E.play(frameOf(SONG_POS)); statusAt = performance.now(); dropsReset();
    playing = true; liveState.setPlaying(true); resumeFollow(); tr.setEdited(false); raf = requestAnimationFrame(tick); say(`playing from ${fmtPos(SONG_POS)}`);
  } else {
    if (E.node) E.stop(); playing = false; liveState.setPlaying(false); cancelAnimationFrame(raf); say(`stopped at ${fmtPos(SONG_POS)}`); setLoad(0);
  }
}
// Stop: first press stops (tails ring), a second one silences and goes back to the loop or song start.
function stopPressed() { if (playing) setPlay(false); else { if (E.node) { E.stop(); E.seek(frameOf(loopOn ? LOOP[0] : 1)); } SONG_POS = loopOn ? LOOP[0] : 1; showPos(); say(`back to ${fmtPos(SONG_POS)}`); } }
E.onStatus = st => { statusAt = performance.now(); meters(st); dropsCheck(); live1.liveProbe(st.probe); loadAvg += (st.load - loadAvg) * .06; setLoad(Math.round(Math.min(1, st.load ? loadAvg : (loadAvg = 0)) * 100)); if (!st.playing && playing && st.gen === E.gen) setPlay(false); };
function tick(now) {
  const st = E.status;
  // between status messages (about every 30 ms) the clock runs on from the last reported frame
  const f = st.position + (st.playing ? (now - statusAt) / 1000 * E.rate : 0);
  let b = barOf(f);
  if (loopOn && b >= LOOP[1] && barOf(st.position) < LOOP[1]) b = LOOP[0] + (b - LOOP[1]);
  if (!loopOn && b >= BARS + 1) { SONG_POS = BARS + 1; showPos(); setPlay(false); return; }
  SONG_POS = b; if (recOn && recHold) recordPoint(recHold.t, recHold.d, recHold.p, curVal(recHold.d, recHold.p.name)); showPos(); applyAutomation(); laneTick(now);
  raf = requestAnimationFrame(tick);
}
// Engine link (ui/app/engine.ts): a seek and every playing frame refresh the arrangement, automation and recording.
initEngine({
  showPos: () => showPos(),
  seeked: () => { applyAutomation(); laneVals.forEach(l => l.val()); },
  frame: now => { recordFrame(); showPos(); applyAutomation(); laneTick(now); },
});
relayout(); updateLoop(); showPos();
queueMicrotask(() => updateArmed());

// ===== Audition (never recorded): computer keyboard on the armed track, piano-roll keys, library entries, Web MIDI =====
const heldV = new Map();
// Mod wheel and pitch bend (sliders, Web MIDI CC1 / pitch bend) set the armed track's instrument controller block
// (unisynth ctl.wheel / ctl.bend) live; never written to the song. A seek or stop restores the stored values.
function ctlLive(name, v) {
  const t = armed, d = t && instOf(t), p = d && paramOf(d, name), di = d ? engineDev(t, d) : -1;
  if (p && di >= 0 && E.node) E.setParam(trackIndex(t), di, p.index, v);
}
function setWheel(v) { au.setMw(v); ctlLive("ctl.wheel", v / 127); }
function setBend(v) { au.setPb(v); ctlLive("ctl.bend", (v + 8192) / 16383); }
function noteOn(t, note, vel, key) {
  if (!t || t.bus || t.master || note < 0 || note > 127 || !instOf(t)) return; noteOff(key);
  const ti = trackIndex(t);
  heldV.set(key, { ti, note }); keyLight(note, true);
  ensureAudio().then(ok => { if (ok && heldV.get(key)?.note === note) E.noteOn(ti, note, vel); });
}
function noteOff(key) { const h = heldV.get(key); if (!h) return; if (E.node) E.noteOff(h.ti, h.note); heldV.delete(key); if (![...heldV.values()].some(x => x.note === h.note)) keyLight(h.note, false); }
// Library: "Audition" plays C3 on the selected track's instrument (load the entry to hear its own sound)
function auditionEntry(src, it, done) {
  if (src.id === "gsm") { gsmOf(it.gsm).then(b64 => b64 && playSample(b64)); return done(); }
  const n = src.id === "gm" && it.gm?.drums ? 36 : 48;
  noteOn(selected, n, au.kbVel(), "lib"); setTimeout(() => { noteOff("lib"); done(); }, 900);
}
// Computer keyboard as a tracker's note keyboard (FastTracker 2 / Renoise layout, by physical key so QWERTZ and
// other layouts play the same): Z S X D C V G B H N J M = one octave from C, Q 2 W 3 E R 5 T 6 Y 7 U I 9 O 0 P = the
// next octave and a half. It plays the selected track (arming follows selection). Octave [ ] (PageDown/PageUp,
// numpad / *), velocity ; ' (numpad − +), ` or the ⌨ button toggles; Shift/Ctrl/Alt combinations stay shortcuts.
// The keys are read by ui/keys.ts (installKeys, below); this holds the state and the actions.
function toggleKb() { const on = !au.kbOn(); au.setKbOn(on); say(on ? "computer keyboard plays the selected track (Z–M, Q–P)" : "computer keyboard off: letters are shortcuts again"); }
// Web MIDI (asks the browser on first use): notes to the armed track, CC1 to MW, pitch bend to PB
function connectMidi() {
  if (!navigator.requestMIDIAccess) { au.setMidiText("MIDI: not available in this browser"); return; }
  navigator.requestMIDIAccess().then(m => {
    const hook = () => { let n = 0; for (const i of m.inputs.values()) { i.onmidimessage = midiMsg; n++; } au.setMidiText(n ? `MIDI: ${n} input${n > 1 ? "s" : ""} → ${armed ? armed.name : "arm a track"}` : "MIDI: no input connected"); };
    hook(); m.onstatechange = hook;
  }).catch(() => { au.setMidiText("MIDI: permission denied"); });
}
function midiMsg(e) {
  const [st, d1, d2] = e.data, cmd = st & 0xf0;
  if (cmd === 0x90 && d2) noteOn(armed, d1, d2, "m" + d1);
  else if (cmd === 0x80 || (cmd === 0x90 && !d2)) noteOff("m" + d1);
  else if (cmd === 0xb0 && d1 === 1) setWheel(d2);
  else if (cmd === 0xe0) setBend(((d2 << 7) | d1) - 8192);
}
// Footer (ui/footer/Audition.tsx): the armed track's name follows renames through the store.
mountAudition(document.getElementById("audition"), { armedName: () => { const id = arrange.armedId(); return id == null ? null : tracks.find(t => t.id === id)?.name ?? null; },
  toggleKb, connectMidi, wheel: setWheel, bend: setBend });
// ===== Files: songs in project/music (/api/music), save with validation, export, import, bounce =====
files.setSongs((DAW.files || []).filter(f => f.kind === "song").map(f => f.path).sort());
let songFile = DAW.file || "project/music/demo.song.json", savedId = -1, savedExtra = "", savedDepth = 0, savedAt = null;
const base = p => p.replace(/^.*\//, "");
// Unsaved = another committed state than at save (history state ids) or song settings that are not undoable changed.
const extraKey = () => tr.tempo() + "|" + tr.sig() + "|" + BARS;
const markSaved = () => { savedId = H.stateId; savedExtra = extraKey(); };
const isDirty = () => H.stateId !== savedId || extraKey() !== savedExtra;
function dirtyUpd() {
  const d = isDirty(), n = Math.max(d ? 1 : 0, Math.abs(H.depth - savedDepth));
  document.title = `${d ? "• " : ""}${base(songFile)} — 64k DAW`;
  files.setDirty({ on: d, n });
  document.getElementById("saved-info").textContent = d ? `${n} unsaved change${n > 1 ? "s" : ""}` : savedAt ? `Saved ${savedAt.toTimeString().slice(0, 5)}` : "No changes";
}
// Puts a song/2 source into the editor (undo history cleared).
function useSong(s, file, saved) {
  if (playing) setPlay(false);
  const st = M.fromSong(s), d = JSON.parse(snap()); setSongMeta(st.meta, st.length0);
  d.tracks = st.tracks; d.sel = (st.tracks.find(t => !t.bus) || st.tracks[0])?.id ?? "master"; d.armed = st.tracks.find(t => t.devices.some(x => x.kind === 64))?.id;
  Object.keys(master).forEach(k => delete master[k]); Object.assign(master, st.master);
  tr.setTempo(st.tempo); const v = st.sig.join("/"); tr.ensureSig(v); document.documentElement.style.setProperty("--beats", beatsPerBar());
  setBars(st.bars); LOOP[0] = 1; LOOP[1] = Math.min(BARS, 4) + 1;
  restore(JSON.stringify(d));
  H.clear(); songFile = file; files.setSongFile(file); files.addSong(file); savedDepth = 0; savedAt = saved ? new Date() : null;
  relayout(); updateLoop(); setPos(1); showPos(); fitSong(true); if (saved) markSaved(); else savedId = -1; unloadLive(); livePush(); dirtyUpd();
}
// A song file as the current layout: a song/1 file migrates (the engine's `song_migrate`), song/2 as it is.
function songText(text) {
  const r = E.migrate(text);
  if (!r.song) throw new Error(r.errors.map(e => `${e.path}: ${e.message}`).join("; "));
  return r.song;
}
async function loadSong(f) {
  try { useSong(songText(await api.read(f)), f, true); say(`opened ${f}`); } catch (e) { say(`cannot open ${f}: ${e.message}`); }
}
function openSong(f) {
  if (f === songFile) return;
  if (!isDirty()) return loadSong(f);
  dialogs.confirmSave(songFile, "Your changes are lost if you switch without saving.", save, () => loadSong(f));
}
// Errors carry their JSON path (crates/song rules, also checked by the server before it writes).
function trackOfPath(path) { const m = path.match(/^\/tracks\/(\d+)/); return m ? tracks[+m[1]] : path.startsWith("/master") ? master : null; }
function showErrors(errs, what) {
  dialogs.showErrors(errs, what, path => { const t = trackOfPath(path); if (!t) return; selectTrack(t); document.querySelector(`.thdr.track[data-track="${t.id}"]`)?.focus(); });
}
function localIssues() {
  const errs = [], seen = new Map();
  tracks.forEach((t, i) => { if (!t.name) errs.push({ path: `/tracks/${i}/name`, message: "track name is empty" }); else if (seen.has(t.name)) errs.push({ path: `/tracks/${i}/name`, message: `duplicate track name "${t.name}"` }); else seen.set(t.name, i); });
  return errs;
}
async function save() {
  const json = M.prettyJson(songOf(false)), errs = [...localIssues(), ...E.validate(json)];
  if (errs.length) { showErrors(errs, "Save"); return false; }
  const r = await api.write(songFile, json);
  if (!r.ok) { if (r.errors) showErrors(r.errors, "Save"); else say(`save failed: ${r.error}`); return false; }
  markSaved(); savedDepth = H.depth; savedAt = new Date(); files.addSong(songFile);
  dirtyUpd(); say(`${api.static ? "downloaded (read-only preview)" : "saved"} ${songFile} (song/2, ${(json.length / 1024).toFixed(1)} kB)`); return true;
}
window.addEventListener("beforeunload", e => { if (isDirty()) { e.preventDefault(); e.returnValue = ""; } });
const emptySong = () => ({ format: "song/2", rate: 44100, tempo: 120, signature: [4, 4], ticks_per_beat: 960, length: 8 * 3840, clips: [],
  tracks: [{ id: "t1", name: "Track 1", color: "--t-lead", volume: 0.5, mute: false, receives: [], devices: [initInstrument()], placements: [], lanes: [] }],
  master: { id: "master", name: "Master", volume: 1, mute: false, receives: [{ from: "t1", channel: "main", volume: 1 }], devices: [], placements: [], lanes: [] } });
function newSong() {
  const go = () => { let n = 1; while (files.songs().includes(`project/music/untitled-${n}.song.json`)) n++; const f = `project/music/untitled-${n}.song.json`;
    useSong(emptySong(), f, false); say(`new song ${base(f)}: one instrument track and the master · double-click the lane to create a clip`); };
  if (!isDirty()) return go();
  dialogs.confirmSave(songFile, "Creating a new song closes this one.", save, go);
}
// Import blob: `song import` in the engine module (devices, routing and automation exact; each lane one clip).
async function importFile(file) {
  const r = E.importBlob(new Uint8Array(await file.arrayBuffer()));
  if (!r.json) return dialogs.notice(`Cannot import ${file.name}`, r.error);
  const f = `project/music/${file.name.replace(/\.(song\.)?bin$/i, "")}.song.json`;
  useSong(JSON.parse(r.json), f, false); say(`imported ${file.name} (${file.size} bytes) → ${base(f)} · save to keep it`);
}
// Export: the blob the intro player reads, written next to the song (same code as `song export`).
async function exportSong() {
  const errs = localIssues(); if (errs.length) return showErrors(errs, "Export");
  const s1 = songOf(false), r = E.exportBlob(JSON.stringify(s1)); if (!r.blob) return showErrors(r.errors, "Export");
  const bin = songFile.replace(/\.song\.json$/, ".song.bin"), notes = s1.tracks.filter(t => !t.mute).reduce((n, t) => n + t.placements.reduce((k, p) => k + (s1.clips.find(c => c.id === p.clip)?.notes.length || 0), 0), 0);
  const sec = BARS / barsPerSec(), all = [...s1.tracks, s1.master];
  dialogs.exportDialog({ bin, kb: (r.blob.length / 1024).toFixed(1), tracks: s1.tracks.length, notes,
    devices: all.reduce((n, t) => n + t.devices.filter(d => !d.bypass).length, 0), lanes: all.reduce((n, t) => n + t.lanes.length, 0),
    length: `${BARS} bars · ${Math.floor(sec / 60)}:${String(Math.round(sec % 60)).padStart(2, "0")} at ${tr.tempo()} BPM · 44.1 kHz` },
    async () => { const w = await api.write(bin, r.blob); say(w.ok ? `exported ${bin} (${(r.blob.length / 1024).toFixed(1)} kB)` : `export failed: ${w.error}`); },
    () => { const a = document.createElement("a"); a.href = URL.createObjectURL(new Blob([M.prettyJson(s1)], { type: "application/json" })); a.download = base(songFile); a.click(); });
}
// Bounce: the exported blob rendered by the song player in a Worker (design D7, equal to `song render`).
function bounceSong() {
  const whole = [1, BARS + 1], fmt = (a, b1) => { const sec = (b1 - a) / barsPerSec(); return `bars ${fmtBars(a)}–${fmtBars(b1)} · ${Math.floor(sec / 60)}:${String(Math.round(sec % 60)).padStart(2, "0")}`; };
  dialogs.bounceDialog({ song: fmt(...whole), loop: fmt(LOOP[0], LOOP[1]) }, (loop, progress) => {
    const [a, b1] = loop ? [LOOP[0], LOOP[1]] : whole;
    const r = E.exportBlob(JSON.stringify(songOf(false))); if (!r.blob) { showErrors(r.errors, "Bounce"); return null; }
    const rate = 44100, range = loop ? [Math.round((a - 1) / barsPerSec() * rate), Math.round((b1 - 1) / barsPerSec() * rate)] : null;
    const job = E.bounce(r.blob, range, progress);
    return { cancel: () => job.cancel(), done: job.done.then(samples => {
      const blob = DAW.wav16(samples, rate), name = base(songFile).replace(/\.song\.json$/, "") + (loop ? `-bars${fmtBars(a)}-${fmtBars(b1)}` : "") + ".wav";
      say(`bounced ${name}`); return { name, url: URL.createObjectURL(blob), seconds: samples.length / 2 / rate, mb: blob.size / 1048576 };
    }) };
  });
}
markSaved(); savedAt = null; files.setSongFile(songFile); files.addSong(songFile); dirtyUpd();
markClean();
DAW.debug = { song: () => songOf(false), selected: () => selected.id, bars: () => BARS, undoDepth: () => H.depth,
  live: () => live1.liveStats() };
// Global shortcuts (ui/keys.ts): Ctrl/⌘+S and the computer keyboard before the focused editor, the rest after it.
installKeys({
  save, undo, redo, toggleKb, kbPlays: () => !!(au.kbOn() && armed),
  kbNoteOn: (i, code) => noteOn(armed, 12 * au.kbOct() + i, au.kbVel(), "k" + code), kbNoteOff: code => noteOff("k" + code),
  octave: d => au.setKbOct(au.kbOct() + d), velocity: d => au.setKbVel(au.kbVel() + 16 * d),
  allNotesOff: () => [...heldV.keys()].forEach(noteOff),
  playPause: () => setPlay(!playing), toHome: () => seek(loopOn ? LOOP[0] : 1), toggleLoop, loopSelection, toggleFollow, toggleRec: () => setRec(!recOn),
  inRoll,
  zoomRoll: f => zoomRoll(f, 1), zoomArr: f => zoomArr(f), fitRoll, fitArr,
});
mountRoll(document.getElementById("p-roll"), { attach: attachRoll, detach: detachRoll });  // last: ui/app/roll.ts has bound its mount setups
}
