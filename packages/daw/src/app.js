// @ts-nocheck
import { batch, createSong, raw, restoreSong } from "./ui/state/song.ts";
import { createHistory } from "./ui/state/history.ts";
import * as liveState from "./ui/state/live.ts";
import * as tr from "./ui/state/transport.ts";
import { mountTransport } from "./ui/transport/TransportBar.tsx";
import * as files from "./ui/state/files.ts";
import * as lib from "./ui/state/library.ts";
import { mountFiles } from "./ui/files/Files.tsx";
import * as dialogs from "./ui/files/dialogs.tsx";
import { mountLibrary, openLibrary } from "./ui/library/Library.tsx";
import { popMenu } from "./ui/common/Menu.tsx";
import * as arrange from "./ui/state/arrange.ts";
import { mountTracks } from "./ui/arrange/Tracks.tsx";  // not `live`: that is the aria-live status element below
import { laneEl, overrides, overrideSet, rollClips, selectedId, setSelectedId } from "./ui/state/view.ts";
import { mountMixer, setMixerShown } from "./ui/mixer.tsx";
// DAW UI (change add-synth-daw): the reviewed mockup's views and interactions on the real song/2 model,
// engine (AudioWorklet + synth-wasm live host), device panels generated from the engine's parameter tables and
// /api/music. Started by main.ts with { engine, model, api, tables, initial, files, library }.
export function start(DAW) {
let BARS = 32, BW = 44; const PAD_BARS = 8;  // song length grows; the view shows 8 bars past the end
let SONG_POS = 1; const LOOP = [1, 5], laneVals = [];  // transport: playhead (bars, 1-based), loop range
// localStorage throws in a sandboxed frame without allow-same-origin (the dashboard canvas): fall back to memory.
const store = (() => { const mem = new Map(); let ls = null; try { ls = window.localStorage; ls.getItem("x"); } catch { ls = null; }
  return { get: k => { try { return ls ? ls.getItem(k) : mem.get(k) ?? null; } catch { return mem.get(k) ?? null; } },
           set: (k, v) => { try { ls ? ls.setItem(k, v) : mem.set(k, v); } catch { mem.set(k, v); } } }; })();
let CAT = null;  // library catalog (/api/library)
const E = DAW.engine, M = DAW.model, api = DAW.api, TABLES = DAW.tables;
const st0 = DAW.initial;
const song = createSong(st0), tracks = song.tracks, master = song.master;  // store proxies (ui/state/song.ts)
BARS = st0.bars; LOOP[1] = Math.min(BARS, 4) + 1;
// ---- device tables (design D8): every device's parameters come from the engine module ----
const choice = (p, n) => Math.min(n - 1, Math.max(0, Math.floor(p * (n - 1) + .5)));
const nModes = k => TABLES.filter(t => t.kind === k).length;
const modeOf = d => { const r = raw(d); return r.kind === 64 ? 0 : choice(r.values.mode ?? 0, nModes(r.kind)); };  // read-only, hot: raw
const tableOf = d => { const k = raw(d).kind, m = modeOf(d); return TABLES.find(t => t.kind === k && t.mode === m); };  // read d once (store proxy)
const paramOf = (d, name) => tableOf(d)?.params.find(p => p.name === name);
const devName = d => d.kind === 64 ? (d.preset || "Unisynth") : tableOf(d)?.label ?? `kind ${d.kind}`;
const instOf = t => t.devices.find(d => d.kind === 64);
const fxOf = t => t.devices.filter(d => d.kind !== 64);
const devicesOf = t => t.devices.map(d => d.kind === 64 ? "Unisynth" : tableOf(d)?.label ?? `kind ${d.kind}`);
const senders = t => tracks.filter(x => x.sends.some(s1 => s1[0] === t.name));
const devLabel = t => t.bus ? `Bus · ← ${senders(t).map(x => x.name).join(", ") || "no sends"}${fxOf(t).length ? " · " + fxOf(t).map(devName).join(" → ") : ""}`
  : t.devices.map(d => devName(d) + (d._edited ? "*" : "") + (d.bypass ? " (off)" : "")).join(" → ") || "no devices";
const links = (t, name) => t.clips.filter(c => c.name === name).length;
// Notes live in a pattern per clip name: linked placements share it, a shorter placement plays its start.
function patOf(t, name) {
  t.pats ??= {};
  if (!t.pats[name]) t.pats[name] = { len: Math.max(1, Math.round(Math.max(...t.clips.filter(c => c.name === name).map(c => c.len), 1))), notes: [], cc: { wheel: [], bend: [] } };
  const pt = t.pats[name]; pt.cc ??= { wheel: [], bend: [] }; return pt;
}
// A clip moved or pasted onto another track brings its notes (never overwrites a pattern already there).
const carry = (from, to, name, pat) => { if (from !== to && !to.pats?.[name]) (to.pats ??= {})[name] = structuredClone(raw(pat || patOf(from, name))); };
let selected = tracks.find(t => !t.bus) || tracks[0] || master, armed = tracks.find(t => instOf(t)) || null;
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
// Colour tokens of style.css (static: no theme switching), cached — getComputedStyle per knob/lane draw cost seconds
// per undo on large songs (bench/ui-profile.ts).
const cssCache = new Map();
const css = v => { let x = cssCache.get(v); if (x === undefined) cssCache.set(v, x = getComputedStyle(document.documentElement).getPropertyValue(v).trim()); return x; };

// Ruler
const ruler = document.getElementById("ruler");
ruler.style.width = BARS * BW + "px";
for (let b = 0; b < BARS; b++) {
  const d = document.createElement("div"); d.className = "bar num"; d.style.left = b * BW + "px"; d.textContent = b + 1; ruler.append(d);
}
const lp = document.createElement("div"); lp.className = "loop"; lp.style.left = 8 * BW + "px"; lp.style.width = 8 * BW + "px"; ruler.append(lp);

// Headers + lanes
const hdrs = document.getElementById("hdrs"), lanes = document.getElementById("lanes");
lanes.style.width = BARS * BW + "px";
function makeClip(t, c) {
  const el = document.createElement("div"), n = links(t, c.name);
  el.className = "clip" + (n > 1 ? " linked" : "") + (c.sel ? " sel" : ""); el.dataset.track = t.id; el._clip = c;
  el.style.left = (c.start - 1) * BW + "px"; el.style.width = c.len * BW - 1 + "px"; el.style.background = `var(${t.col})`;
  el.tabIndex = -1; el.setAttribute("role", "button"); el.setAttribute("aria-label", `Clip ${c.name}, bar ${+c.start.toFixed(2)}, ${+c.len.toFixed(2)} bars${c.loop ? ", looping" : ""}${n > 1 ? ", linked" : ""}${c.sel ? ", selected" : ""}`);
  el.innerHTML = `<div class="lbl">${c.name}</div><div class="notes"></div><div class="rz" title="Drag: crop or extend · Alt+drag: loop the clip"></div>`;
  const nb = el.querySelector(".notes"), pt = t.bus ? null : patOf(t, c.name), ns = pt ? raw(pt.notes) : [],  // read-only: raw (no proxy per note)
    steps = c.len * 16, per = c.loop ? pt.len * 16 : Infinity;
  if (c.loop) { el.querySelector(".lbl").append(" ⟳"); for (let x = per; x < steps; x += per) { const m = document.createElement("b"); m.className = "lpm"; m.style.left = (x / steps) * 100 + "%"; nb.append(m); } }
  if (ns.length) { const lo = Math.min(...ns.map(n => n.n)), hi = Math.max(...ns.map(n => n.n)), span = Math.max(1, hi - lo);
    for (let k = 0; k === 0 || (c.loop && k * per < steps); k++) for (const n of ns) { const s0 = n.s + k * (c.loop ? per : 0); if (s0 >= steps || (c.loop && n.s >= per)) continue;
      const i = document.createElement("i"); i.style.left = (s0 / steps) * 100 + "%"; i.style.width = Math.max(1.5, (Math.min(n.l, steps - s0) / steps) * (c.len * BW - 4)) + "px";
      i.style.top = ((hi - n.n) / span) * 88 + "%"; nb.append(i); } }
  return el;
}
// Every note occurrence of a placement: [note, step in the clip, audible length]. A looping clip repeats its
// pattern (pattern length = pt.len bars); a cropped clip plays the pattern's start.
function* clipNotes(t, c) {
  const pt = patOf(t, c.name), steps = c.len * 16, per = c.loop ? pt.len * 16 : Infinity;
  for (let k = 0; k === 0 || (c.loop && k * per < steps); k++) for (const n of raw(pt.notes)) {
    if (c.loop && n.s >= per) continue; const s0 = n.s + k * (c.loop ? per : 0); if (s0 < steps) yield [n, s0, Math.min(n.l, steps - s0)];
  }
}
function renderLane(t) {
  const lane = laneEl.get(t.id); if (!lane || t.bus) return;
  lane.querySelectorAll(".clip").forEach(e => e.remove());
  for (const c of t.clips) lane.append(makeClip(t, c));
}
// Laws from docs/synth/unisynth.md
const L = {
  cutoff: p => { const v = 20 + 19980 * p * p; return v >= 1000 ? (v / 1000).toFixed(2) + "k" : v.toFixed(0) + " Hz"; },
  pct: p => (p * 100).toFixed(0) + "%",
  ms: p => { const v = p * p * 5000 + 1; return v >= 1000 ? (v / 1000).toFixed(2) + " s" : v.toFixed(0) + " ms"; },
  gain: p => { const g = 16 * p * p; return g <= 0 ? "−∞ dB" : (20 * Math.log10(g)).toFixed(1) + " dB"; },
  semi: p => { const v = Math.round((p - .5) * 48); return (v > 0 ? "+" : "") + v + " st"; },
  fine: p => { const v = (p - .5) * 2; return (v >= 0 ? "+" : "") + v.toFixed(2); },
  oct: p => { const v = Math.round((p - .5) * 8); return (v > 0 ? "+" : "") + v; },
  hz: p => (0.01 * Math.pow(10, 4 * p)).toFixed(2) + " Hz",
  pan: p => p === .5 ? "C" : (p < .5 ? "L" + Math.round((.5 - p) * 200) : "R" + Math.round((p - .5) * 200)),
  fb: p => (8 * p * p).toFixed(2),
  int: n => p => String(1 + Math.round(p * (n - 1))),
};
// Automation targets: every non-structural parameter of the track's devices (engine tables).
const autoParams = d => (tableOf(d)?.params || []).filter(p => !p.structural && !p.raw);
// Physical value text of a device parameter (laws from the tables; the engine computes the value).
function fmtVal(d, p, v) {
  if (p.law === "Index") return p.options[Math.round(v * 255)] ?? String(Math.round(v * 255));
  if (p.options.length) return p.options[choice(v, p.options.length)] ?? String(v);
  if (p.raw) return String(Math.round(v));
  if (/^osc\d\.s[034]$/.test(p.name) && oscType(d, +p.name[3]) === 4) return (v * 100).toFixed(1) + " %";
  const x = E.physical(d.kind, modeOf(d), p.index, v), law = p.law;
  if (law === "Cutoff" || law === "FixedHz" || law === "LfoRate") return x >= 1000 ? (x / 1000).toFixed(2) + "k" : x.toFixed(x < 10 ? 2 : 0) + " Hz";
  if (law === "Gain") return x <= 0 ? "−∞ dB" : (20 * Math.log10(x)).toFixed(1) + " dB";
  if (law === "Time") return x.toFixed(x < 1 ? 3 : 2) + " s";
  if (law === "EnvTime") {
    const m = /^env(\d)\.(attack|decay|release)$/.exec(p.name), tf = m && d.kind === 64 && envRate(vget(d, ip(`env${m[1]}.law`)));
    const s = tf ? tfEnvSeconds(m[2], v, vget(d, ip(`env${m[1]}.sustain`)), vget(d, ip(`env${m[1]}.curve_slope`))) : envMs(v) / 1000;
    return !isFinite(s) ? (m[2] === "decay" ? "holds" : "∞") : s >= 1 ? s.toFixed(2) + " s" : Math.round(s * 1000) + " ms";
  }
  if (law === "Transpose" || law === "Coarse") return (x > 0 ? "+" : "") + Math.round(x) + " st";
  if (law === "Fine") return (x >= 0 ? "+" : "") + x.toFixed(2) + " st";
  if (law === "Octave") return (x > 0 ? "+" : "") + x;
  if (law === "Ratio" || law === "RatioFine") return x.toFixed(3) + "×";
  if (law === "Feedback" || law === "PmOut") return x.toFixed(2);
  if (law === "Pan") return L.pan(v);
  if (law.startsWith("Count")) return String(x);
  return (v * 100).toFixed(0) + "%";
}
const laneText = (t, ln, v) => { const d = t.devices[ln.dev], p = d && paramOf(d, ln.param); return p ? fmtVal(d, p, v) : (v * 100).toFixed(0) + "%"; };
function laneValue(pts, bar) {
  pts = raw(pts);  // read-only, runs per lane per frame
  if (bar <= pts[0][0]) return pts[0][1];
  for (let i = 1; i < pts.length; i++) if (bar <= pts[i][0]) { const [b0, v0] = pts[i - 1], [b1, v1] = pts[i]; return v0 + (v1 - v0) * (bar - b0) / (b1 - b0); }
  return pts[pts.length - 1][1];
}
// Knob values live in stores on the track (slot items, t.vals.voice/noise, t.fxv[i]); knobReg finds the knob
// element of a store field so automation can move it. An automated knob turned by hand is "overridden" and
// stops following its lane until re-enabled (Ableton's rule).
const knobReg = new WeakMap(), autoVal = new WeakMap();  // autoVal: store → { field: value at the playhead } (view state, not saved)
function paramTarget(t, ln) { const d = t.devices[ln.dev]; return d && paramOf(d, ln.param) ? { store: d.values, field: ln.param } : null; }
function applyAutomation() {
  for (const t of tracks) for (const ln of t.lanes || []) {
    const tg = paramTarget(t, ln); if (!tg) continue;
    const key = ln.dev + ":" + ln.param, ovr = !!overrides.get(t.id)?.has(key), el = knobReg.get(tg.store)?.get(tg.field);
    const av = autoVal.get(tg.store) || autoVal.set(tg.store, {}).get(tg.store);
    // write a knob only when its value or override state changed (this runs every frame while playing)
    if (!ovr) { const nv = laneValue(ln.pts, SONG_POS), moved = av[tg.field] !== nv; av[tg.field] = nv; if (moved && el?.isConnected) el._set(nv); } else delete av[tg.field];
    if (el?.isConnected && (el._lane?.key !== key || el._ovr !== ovr)) { el._lane = { t, key }; el._ovr = ovr; el.classList.add("auto"); el.classList.toggle("ovr", ovr); el.title = ovr ? "Automation overridden by hand · Re-enable automation to follow the lane again" : "Automated: follows its lane"; }
  }
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
function knobEdited(t, el) {
  if (el._lane) { overrideSet(el._lane.t.id).add(el._lane.key); el.classList.add("ovr"); applyAutomation(); updateReauto(el._lane.t); }
  const d = el._dev; if (d && d.kind === 64 && !d._edited) {
    d._edited = true; const k = el.closest(".devbox")?.querySelector("h3 .kind"); if (k) k.textContent = k.textContent.replace(/(preset [^·]*?)(\s*·|$)/, (m0, n, rest) => `${n.replace(/\*$/, "")}*${rest}`);
    arrange.setLabelRev(n => n + 1);  // the header shows the * on the edited preset
  }
}
/** One line from a song/2 about: type · style · tempo · engine · like … — description. */
const aboutText = a => [a.type, a.style, a.tempo, a.engine, a.similar_to?.length ? "like " + a.similar_to.join(", ") : ""].filter(Boolean).join(" · ") + " — " + a.description;
const live = document.getElementById("live");
function addLane(t, ln, box, beforeH, beforeL) {
  // the target list (every parameter of every device, hundreds of options) is built when the select is first used;
  // until then it holds the current target only (rows mount while scrolling: design D6)
  const names = devicesOf(t), opts = () => { const used = new Set(t.lanes.filter(x => x !== ln).map(x => x.dev + ":" + x.param));
    return t.devices.map((dv, di) => `<optgroup label="${names[di]}">${autoParams(dv).map(p =>
      `<option value="${di}:${p.name}"${di === ln.dev && p.name === ln.param ? " selected" : ""}${used.has(di + ":" + p.name) ? " disabled" : ""}>${names[di]} · ${p.name}</option>`).join("")}</optgroup>`).join(""); };
  const ah = document.createElement("div"); ah.className = "thdr auto";
  const cur = () => laneText(t, ln, laneValue(ln.pts, SONG_POS));
  ah.innerHTML = `<label class="tgt"><span aria-hidden="true">↳</span><select aria-label="${t.name} automation target"><option value="${ln.dev}:${ln.param}" selected>${names[ln.dev]} · ${ln.param}</option></select></label>
    <span class="num val" title="Value at the playhead">${cur()}</span>
    <button class="rm" aria-label="Remove ${devicesOf(t)[ln.dev]} ${ln.param} lane" title="Remove lane">✕</button>`;
  const al = document.createElement("div"); al.className = "lane auto"; al.setAttribute("role", "group"); al.setAttribute("aria-label", `${t.name} ${devicesOf(t)[ln.dev]} ${ln.param} automation`);
  // Point editor. Mouse: click empty = add, drag point = move (snap 1/16 bar, Alt = free; time stays between
  // neighbours), double-click / right-click point = delete. Keyboard (point focused): ←/→ time, ↑/↓ value
  // (Shift = fine), Delete/Backspace = delete. Every gesture is one undo step in the real app.
  const H = 40, X = b => (b - 1) * BW, Y = v => H - 4 - v * (H - 8);
  let selI = -1, drag = null, lastDown = null;
  const fmtBar = b => { const t = Math.round((b - 1) * 16); return `${1 + Math.floor(t / 16)}.${1 + Math.floor((t % 16) / 4)}.${(t % 4) * 240}`; };
  const draw = (focus = false) => {
    const lp = raw(ln.pts), dname = devicesOf(t)[ln.dev];  // read-only: raw points, name once (not per point)
    const W = (BARS + PAD_BARS) * BW;
    const pts = [[1, lp[0][1]], ...lp, [BARS + 1, lp[lp.length - 1][1]]];
    const c = css(t.col);
    al.innerHTML = `<svg width="${W}" height="${H}"><polygon fill="${c}1f" points="${X(1)},${H} ${pts.map(([b, v]) => `${X(b)},${Y(v)}`).join(" ")} ${X(BARS + 1)},${H}"/><polyline fill="none" stroke="${c}" stroke-width="1.5" points="${pts.map(([b, v]) => `${X(b)},${Y(v)}`).join(" ")}"/>${
      lp.map(([b, v], i) => `<g class="pt${i === selI ? " sel" : ""}" data-i="${i}" tabindex="0" role="slider" aria-label="${dname} ${ln.param} point ${i + 1} at ${fmtBar(b)}" aria-valuemin="0" aria-valuemax="1" aria-valuenow="${v.toFixed(3)}" aria-valuetext="${laneText(t, ln, v)} at bar ${fmtBar(b)}"><circle class="hit" cx="${X(b)}" cy="${Y(v)}" r="8"/><circle class="dot" cx="${X(b)}" cy="${Y(v)}" r="${i === selI ? 4 : 3}" fill="${i === selI ? c : css("--bg")}" stroke="${c}"/></g>`).join("")
    }${drag && drag.moved ? (() => { const [b, v] = lp[drag.i]; const tx = Math.min(W - 120, X(b) + 8); return `<rect x="${tx - 3}" y="2" width="118" height="14" rx="3" fill="${css("--panel")}" stroke="${css("--line")}"/><text x="${tx}" y="12" font-size="10" fill="${css("--fg")}" font-family="ui-monospace,monospace">${fmtBar(b)} · ${laneText(t, ln, v)}</text>`; })() : ""}</svg>`;
    ah.querySelector(".val").textContent = cur(); applyAutomation();
    if (focus && selI >= 0) al.querySelector(`.pt[data-i="${selI}"]`)?.focus();
  };
  laneVals.push({ box, draw, t, ln, ah, val: () => { ah.querySelector(".val").textContent = cur(); } }); draw();
  const toBV = (e, free) => {
    const r = al.getBoundingClientRect();
    const raw = 1 + (e.clientX - r.left) / BW, b = free ? raw : 1 + Math.round((raw - 1) * 16) / 16;
    return [Math.min(BARS + 1, Math.max(1, b)), Math.min(1, Math.max(0, (r.top + H - 4 - e.clientY) / (H - 8)))];
  };
  const clampTime = (i, b) => {
    const lo = i > 0 ? ln.pts[i - 1][0] + 1 / 64 : 1, hi = i < ln.pts.length - 1 ? ln.pts[i + 1][0] - 1 / 64 : BARS + 1;
    return Math.min(hi, Math.max(lo, b));
  };
  const remove = i => { const before = snap(); setTimeout(() => commit(before, `${ln.param}: point deleted`)); if (ln.pts.length === 1) { status2(`${ln.param}: last point kept; remove the lane with ✕ instead`); return; } ln.pts.splice(i, 1); selI = Math.min(i, ln.pts.length - 1); draw(true); status2(`point deleted`); };
  const status2 = m => { live.textContent = `${t.name} ${ln.param}: ${m}`; };
  al.addEventListener("pointerdown", e => {
    if (e.button !== 0) return;
    const g = e.target.closest(".pt");
    al.setPointerCapture(e.pointerId);
    if (g) {
      const i = +g.dataset.i, now = performance.now();
      if (lastDown && lastDown.i === i && now - lastDown.t < 400) { lastDown = null; drag = null; remove(i); return; }  // double-click
      lastDown = { i, t: now };
      selI = i; drag = { i, moved: false, x0: e.clientX, y0: e.clientY, before: snap() }; draw();
    }
    else drag = { add: true, x0: e.clientX, y0: e.clientY, before: snap() };
  });
  al.addEventListener("pointermove", e => {
    if (!drag || drag.add) return;
    if (!drag.moved && Math.hypot(e.clientX - drag.x0, e.clientY - drag.y0) < 3) return;
    drag.moved = true;
    const [b, v] = toBV(e, e.altKey);
    ln.pts[drag.i] = [clampTime(drag.i, b), v]; draw();
  });
  al.addEventListener("pointerup", e => {
    if (drag?.add && Math.hypot(e.clientX - drag.x0, e.clientY - drag.y0) < 3) {
      const [b, v] = toBV(e, e.altKey);
      ln.pts.push([b, v]); ln.pts.sort((p1, p2) => p1[0] - p2[0]); selI = ln.pts.findIndex(p => p[0] === b && p[1] === v); status2(`point added at ${fmtBar(b)}`);
    } else if (drag?.moved) status2(`point moved to ${fmtBar(ln.pts[drag.i][0])}, ${laneText(t, ln, ln.pts[drag.i][1])}`);
    if (drag?.before) commit(drag.before, live.textContent);
    drag = null; draw(true);
  });
  al.addEventListener("contextmenu", e => { const g = e.target.closest(".pt"); if (g) { e.preventDefault(); remove(+g.dataset.i); } });
  al.addEventListener("focusin", e => { const g = e.target.closest(".pt"); if (g && +g.dataset.i !== selI) { selI = +g.dataset.i; draw(true); } });
  al.addEventListener("keydown", e => {
    const g = e.target.closest(".pt"); if (!g) return;
    const i = +g.dataset.i; let [b, v] = ln.pts[i];
    const dv = e.shiftKey ? 0.005 : 0.05, db = e.shiftKey ? 1 / 64 : 1 / 16;
    if (e.key === "ArrowUp") v = Math.min(1, v + dv); else if (e.key === "ArrowDown") v = Math.max(0, v - dv);
    else if (e.key === "ArrowRight") b = clampTime(i, b + db); else if (e.key === "ArrowLeft") b = clampTime(i, b - db);
    else if (e.key === "Delete" || e.key === "Backspace") { e.preventDefault(); remove(i); return; }
    else return;
    e.preventDefault(); const before = snap(); ln.pts[i] = [b, v]; selI = i; draw(true); commit(before, `${ln.param} point ${fmtBar(b)} · ${laneText(t, ln, v)}`);
  });
  { const sl = ah.querySelector(".tgt select"); let full = false;
    const fill = () => { if (full) return; full = true; sl.innerHTML = opts(); };
    for (const ev of ["pointerdown", "focus", "keydown"]) sl.addEventListener(ev, fill); }
  ah.querySelector(".tgt select").addEventListener("change", e => {
    const [di, p] = e.target.value.split(":"), before = snap(); ln.dev = +di; ln.param = p; draw(); commit(before, `${t.name}: lane now ${devicesOf(t)[ln.dev]} · ${p}`);
  });
  ah.querySelector(".rm").addEventListener("click", () => {
    const before = snap(); t.lanes.splice(t.lanes.indexOf(ln), 1); rebuildTracks(); commit(before, `${t.name}: lane ${ln.param} removed`);
  });
  if (beforeH) { box.h.insertBefore(ah, beforeH); box.l.insertBefore(al, beforeL); } else { box.h.append(ah); box.l.append(al); }
}
// Track rows: ui/arrange/Tracks.tsx (windowed, design D6). A mounted row asks for its clips (mountLane) and, while
// open, its automation lanes (mountAuto); rebuildTracks() bumps arrange.rev so mounted rows refill after
// add/delete/reorder/undo. Rows off screen hold nothing (laneEl / laneVals only list mounted rows).
function mountLane(t, el) {
  laneEl.set(t.id, el); renderLane(t);
  return () => { if (laneEl.get(t.id) === el) laneEl.delete(t.id); el.querySelectorAll(".clip").forEach(e => e.remove()); };
}
function mountAuto(t, box) {
  for (const ln of t.lanes) addLane(t, ln, box);
  const addH = document.createElement("div"); addH.className = "thdr addlane";
  addH.innerHTML = `<button aria-label="Add automation lane to ${t.name}">+ lane</button><span class="dim">one lane per parameter</span>`;
  const addL = document.createElement("div"); addL.className = "lane addlane";
  addH.querySelector("button").addEventListener("click", () => {
    const used = new Set(t.lanes.map(x => x.dev + ":" + x.param));
    const pref = ["filt1.cutoff", "filt1.resonance", "master", "dry_wet", "feedback"];
    const cand = t.devices.flatMap((dv, di) => autoParams(dv).map(p => [di, p])).filter(([di, p]) => !used.has(di + ":" + p.name));
    const free = cand.find(([, p]) => pref.includes(p.name)) || cand[0];
    if (!free) return live.textContent = `${t.name}: ${t.devices.length ? "every automatable parameter already has a lane" : "add a device first"}`;
    const [di, p] = free, d = t.devices[di], before = snap(), ln = { dev: di, param: p.name, pts: [[1, d.values[p.name] ?? p.default]] };
    t.lanes.push(ln); addLane(t, ln, box, addH, addL); commit(before, `${t.name}: lane ${devicesOf(t)[di]} · ${p.name} added`);
  });
  box.h.append(addH); box.l.append(addL);
  return () => { for (let i = laneVals.length; i--;) if (laneVals[i].box === box) laneVals.splice(i, 1); };
}
function rebuildTracks() {
  arrange.setRev(v => v + 1);
  if (typeof relayout === "function") { relayout(); applyAutomation(); hdrPad(); hdrs.scrollTop = lw.scrollTop; }
}
const shade = document.createElement("div"); shade.className = "loopshade"; shade.style.left = 8 * BW + "px"; shade.style.width = 8 * BW + "px"; lanes.append(shade);
const ph = document.createElement("div"); ph.className = "playhead"; ph.style.transform = `translateX(${(11 + 2.5 / 4) * BW}px)`; lanes.append(ph);
const ph2 = ph.cloneNode(); ruler.append(ph2);
const lw = document.getElementById("lanes-wrap");
arrange.setArmedId(armed?.id ?? null); setSelectedId(selected.id);  // the rows mark and pin them from the start
mountTracks(hdrs, lanes, lw, tracks, {
  devLabel: t => devLabel(t), about: t => t.about ? aboutText(t.about) : "", rename: (t, v) => renameTo(t, v),
  mountLane: (t, el) => mountLane(t, el), mountAuto: (t, box) => mountAuto(t, box),
});
lw.addEventListener("scroll", () => { ruler.style.transform = `translateX(${-lw.scrollLeft}px)`; hdrs.scrollTop = lw.scrollTop; });
// One scroller: the lanes. Headers only mirror lw.scrollTop (they never write back, which made momentum scrolling
// jump back a frame). Wheel over the headers scrolls the lanes; focusing a header scrolls the lanes to it.
const hdrPad = () => { hdrs.style.paddingBottom = (lw.offsetHeight - lw.clientHeight) + 40 + "px"; };  // room for lw's horizontal scrollbar
hdrPad(); new ResizeObserver(hdrPad).observe(lw);
hdrs.addEventListener("wheel", e => { if (e.ctrlKey || e.metaKey) return; e.preventDefault(); lw.scrollTop += e.deltaY; lw.scrollLeft += e.deltaX; }, { passive: false });
hdrs.addEventListener("focusin", e => {
  const h = e.target.closest(".thdr"); if (!h) return; const top = h.offsetTop, bot = top + h.offsetHeight;
  if (top < lw.scrollTop) lw.scrollTop = top; else if (bot > lw.scrollTop + lw.clientHeight) lw.scrollTop = bot - lw.clientHeight;
  requestAnimationFrame(() => { hdrs.scrollTop = lw.scrollTop; });
});

// Knob factory: SVG arc, role=slider, keyboard + drag.
function knob(name, p, text, opts = {}) {
  const store = opts.store, field = opts.field ?? name;
  if (store && typeof store[field] === "number") p = store[field];
  if (store && typeof autoVal.get(store)?.[field] === "number") p = autoVal.get(store)[field];  // automated: show the lane value
  const el = document.createElement("div"); el.className = "knob" + (opts.mod ? " mod" : "");
  el.tabIndex = 0; el.setAttribute("role", "slider"); el.setAttribute("aria-label", name);
  el.setAttribute("aria-valuemin", "0"); el.setAttribute("aria-valuemax", "1");
  const draw = () => {
    const a0 = -135, a = a0 + 270 * p, r = 12, c = 15;
    const pt = d => [c + r * Math.sin(d * Math.PI / 180), c - r * Math.cos(d * Math.PI / 180)];
    const [x0, y0] = pt(a0), [x1, y1] = pt(a), [x2, y2] = pt(135);
    el.setAttribute("aria-valuenow", p.toFixed(3)); el.setAttribute("aria-valuetext", text(p));
    el.innerHTML = `<svg viewBox="0 0 30 30" aria-hidden="true">
      <path d="M${x0} ${y0} A${r} ${r} 0 1 1 ${x2} ${y2}" fill="none" stroke="${css("--line")}" stroke-width="3" stroke-linecap="round"/>
      <path class="arc" d="M${x0} ${y0} A${r} ${r} 0 ${a - a0 > 180 ? 1 : 0} 1 ${x1} ${y1}" fill="none" stroke="${css("--focus")}" stroke-width="3" stroke-linecap="round"/>
      <line x1="${c}" y1="${c}" x2="${x1}" y2="${y1}" stroke="${css("--fg")}" stroke-width="1.5"/></svg>
      <span class="v num">${text(p)}</span><span class="n">${name}</span>`;
  };
  const def = opts.def ?? p;
  const changed = () => { draw(); if (store) store[field] = p; opts.onChange?.(p, el); redrawGroup(el); };
  el._value = () => p;
  el._redrawText = () => draw();
  el._set = v => { p = v; draw(); opts.onSet?.(p, el); redrawGroup(el); };
  if (store) (knobReg.get(store) || knobReg.set(store, new Map()).get(store)).set(field, el);
  el.addEventListener("keydown", e => {
    const st = e.shiftKey ? 0.002 : 0.01, before = snap();
    queueMicrotask(() => commit(before, `${name} ${text(p)}`));
    if (e.key === "ArrowUp" || e.key === "ArrowRight") p = Math.min(1, p + st);
    else if (e.key === "ArrowDown" || e.key === "ArrowLeft") p = Math.max(0, p - st);
    else if (e.key === "Home") p = 0; else if (e.key === "End") p = 1; else return;
    e.preventDefault(); changed();
  });
  el.addEventListener("dblclick", () => { const before = snap(); p = def; changed(); commit(before, `${name} reset to ${text(p)}`); });
  el.addEventListener("pointerdown", e => {
    el.setPointerCapture(e.pointerId); const y0 = e.clientY, p0 = p, before = snap(); opts.onGrab?.(el);
    const mv = ev => { p = Math.min(1, Math.max(0, p0 + (y0 - ev.clientY) / (ev.shiftKey ? 800 : 160))); changed(); };
    el.addEventListener("pointermove", mv); el.addEventListener("pointerup", () => { el.removeEventListener("pointermove", mv); opts.onRelease?.(el); commit(before, `${name} ${text(p)}`); }, { once: true });
  });
  draw(); return el;
}
// A group's graphic (envelope curve, sample view) follows its knobs: dragging, keys, undo-free automation playback.
const redrawQ = new Set();
function redrawGroup(el) {
  const g = el.closest?.(".grp"); if (!g?._redraw) return;
  if (!redrawQ.size) requestAnimationFrame(() => { const q = [...redrawQ]; redrawQ.clear(); q.forEach(x => x._redraw()); });
  redrawQ.add(g);
}
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
// (`d.values`, by parameter name); a knob or select writes there, sets the live parameter at once and queues an
// engine update (structural parameters rebuild the device there). ----
const status = document.createElement("div"); status.className = "rack-status"; status.setAttribute("aria-live", "polite");
function INST() { return TABLES.find(t => t.kind === 64); }
function ip(name) { return INST().params.find(p => p.name === name); } // hoisted: fmtVal may run before this line
function vget(d, p) { return d.values[p.name] ?? p.default; }
/** An envelope `law` value plays the Tunefish rate law (its option, by stored position: the law gained V2's kind). */
function envRate(v) { const o = ip("env1.law").options; return o[choice(v, o.length)] === "Rate"; }
const trackIndex = t => t.master ? tracks.length : tracks.indexOf(t);
const engineDev = (t, d) => t.devices.filter(x => !x.bypass).indexOf(d);
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
const OSC_COMMON = ["tracking", "coarse", "fine", "octave", "ratio", "ratio_fine", "fixed_hz", "gain", "mix", "pan", "pm_out", "feedback"];
const SLOTS = { osc: 3, filt: 4, env: 4, lfo: 2, mod: 12 };
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
// Slot edits rewrite mod-matrix targets and automation lanes in one command (design D8a). `map(slot)` → new slot
// number or 0 (gone) for every slot of group g.
const TARGETS = () => ip("mod1.target").options;
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
const SLOT_NOUN = { osc: "Osc", filt: "Filter", env: "Env", lfo: "LFO" };
const slotOpen = new Map();
const blkClosed = k => store.get(`daw.blk.${k}`) === "1";
// a mod row's source name ("None", "Env 1", "LFO 2", "Velocity", V2 CCs …) from the parameter table
function modSrcName(d, m) { const p = ip(`mod${m}.source`); return p.options[choice(vget(d, p), p.options.length)] || "None"; }
// mod targets an envelope or LFO slot routes to (by its mod rows)
function routesOf(d, g, i) {
  const src = `${g === "env" ? "Env" : "LFO"} ${i}`, out = [];
  for (let m = 1; m <= SLOTS.mod; m++) if (modSrcName(d, m) === src) out.push(TARGETS()[Math.round(vget(d, ip(`mod${m}.target`)) * 255)] || "?");
  return out;
}
function slotCmd(t, msg, fn) { const before = snap(); fn(); rebuildTracks(); renderDevice(); status.textContent = msg; commit(before, msg); }
function slotCard(t, d, g, i, el) {
  const noun = SLOT_NOUN[g], title = `${noun} ${i}`, used = slotUsed(d, g, i), key = `${t.id}:${g}${i}`, closed = !(slotOpen.get(key) ?? used);
  el.classList.add("slot"); el.classList.toggle("off", !used); el.classList.toggle("closed", closed);
  const gh = el.querySelector(".gh"), tw = document.createElement("button");
  tw.className = "tw"; tw.textContent = closed ? "▸" : "▾"; tw.setAttribute("aria-expanded", !closed); tw.setAttribute("aria-label", `${closed ? "Expand" : "Collapse"} ${title}`);
  tw.onclick = () => { slotOpen.set(key, closed); renderDevice(); };
  gh.prepend(tw);
  const sp = document.createElement("span"); sp.className = "sp"; gh.append(sp);
  if (g === "env" && i === 1) gh.insertAdjacentHTML("beforeend", `<span class="dim" title="Envelope 1 shapes the amplitude and ends the voice">fixed</span>`);
  else if (used) {
    const x = document.createElement("button"); x.className = "clr"; x.textContent = "✕"; x.title = `Reset ${title} (later slots stay where they are)`; x.setAttribute("aria-label", `Clear ${title}`);
    x.onclick = () => slotCmd(t, `${title} cleared; mod rows that targeted it are off, its lanes removed`, () => { remapSlots(t, d, g, y => y === i ? 0 : y); slotOpen.delete(key); });
    gh.append(x);
  }
  el.querySelector("svg.scr, svg.env")?.setAttribute("data-live", `${g}${i}`);
  if (g === "env" || g === "lfo") {
    const r = g === "env" && i === 1 ? ["amplitude"] : routesOf(d, g, i), rt = document.createElement("div"); rt.className = "rt";
    rt.textContent = r.length ? `→ ${r.join(", ")}` : "unrouted · knobs inactive"; el.classList.toggle("unrouted", !r.length);
    gh.after(rt);
  }
  return el;
}
function slotBlock(k, label, n, items) {
  const closed = blkClosed(k), b = document.createElement("div"); b.className = "blk" + (closed ? " closed" : ""); b.dataset.k = k;
  b.setAttribute("role", "group"); b.setAttribute("aria-label", label);
  b.innerHTML = `<div class="blk-h"><button class="tw" aria-expanded="${!closed}" aria-label="${closed ? "Expand" : "Collapse"} ${label}">${closed ? "▸" : "▾"}</button><span>${label}</span><span class="cnt num">${n}</span></div>`;
  b.querySelector(".tw").onclick = () => { store.set(`daw.blk.${k}`, closed ? "0" : "1"); renderDevice(); };
  const list = document.createElement("div"); list.className = "blk-list"; list.append(...items); b.append(list); return b;
}
const usedCount = (d, g) => `${[...Array(SLOTS[g])].filter((_, i) => slotUsed(d, g, i + 1)).length}/${SLOTS[g]}`;
// filters: a ⇅ handle between neighbours swaps them (mod targets and lanes follow)
function filterItems(t, d, card) {
  const out = [];
  for (let i = 1; i <= SLOTS.filt; i++) {
    if (i > 1) {
      const sw = document.createElement("button"); sw.className = "swap"; sw.textContent = "⇅";
      sw.title = `Swap filter ${i - 1} and ${i} (serial order; mod targets and lanes follow)`; sw.setAttribute("aria-label", `Swap Filter ${i - 1} and Filter ${i}`);
      sw.onclick = () => slotCmd(t, `Filter ${i - 1} ↔ ${i}: mod targets and automation lanes follow`, () => {
        remapSlots(t, d, "filt", x => x === i ? i - 1 : x === i - 1 ? i : x);
        const a = slotOpen.get(`${t.id}:filt${i - 1}`), b = slotOpen.get(`${t.id}:filt${i}`);
        [[i - 1, b], [i, a]].forEach(([j, v]) => v === undefined ? slotOpen.delete(`${t.id}:filt${j}`) : slotOpen.set(`${t.id}:filt${j}`, v)); });
      out.push(sw);
    }
    out.push(card(i));
  }
  return out;
}
// mod matrix: one table row per slot (source, target, operation, amount); rows with source None dimmed
function modTable(t, d) {
  const tb = document.createElement("table"); tb.className = "mm";
  tb.innerHTML = `<thead><tr><th></th><th>source</th><th>target</th><th>op</th><th>amount</th></tr></thead><tbody></tbody>`;
  for (let m = 1; m <= SLOTS.mod; m++) {
    const src = modSrcName(d, m), tr = document.createElement("tr"); tr.className = src === "None" ? "off" : "";
    tr.dataset.src = src === "None" ? "" : /^Env/.test(src) ? "env" : /^LFO/.test(src) ? "lfo" : "voice";
    tr.innerHTML = `<td class="n num">${m}</td>`;
    for (const f of ["source", "target", "operation", "amount"]) { const td = document.createElement("td"); td.append(pctl(t, d, ip(`mod${m}.${f}`), "")); tr.append(td); }
    // a source change re-routes: rebuild for the row dimming and the env/LFO routing hints
    tr.querySelector(`select[aria-label$=" mod${m}.source"]`)?.addEventListener("change", () => renderDevice());
    tr.querySelector(`select[aria-label$=" mod${m}.target"]`)?.addEventListener("change", () => renderDevice());
    tb.tBodies[0].append(tr);
  }
  return tb;
}
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
function envMs(p) { return p * p * 5000 + 1; } // hoisted (like ip/vget/INST/oscType): fmtVal runs from the first mounted row (mountTracks), above
function envCurve(tf, a, dcy, s, r, slope) {
  const pts = [];
  if (!tf) {
    const A = envMs(a) / 1000, D = envMs(dcy) / 1000, R = envMs(r) / 1000, hold = Math.max(0.05, 0.25 * (A + D));
    pts.push([0, 0], [A, 1]);
    for (let k = 1; k <= 24; k++) { const f = (1 - k / 24) ** 2; pts.push([A + D * k / 24, f + s * (1 - f)]); }
    const off = A + D + hold; pts.push([off, s]);
    for (let k = 1; k <= 24; k++) pts.push([off + R * k / 24, s * (1 - k / 24) ** 2]);
    return { pts, off };
  }
  const B = 512 / 44100, STEP = 0.0005 * 512, cube = x => x * x * x, sus = Math.min(s, 0.99), MAX = 20 / B;
  let v = 0, n = 0, stage = 0; pts.push([0, 0]);
  const up = -Math.log10(Math.max(cube(a), 1e-9) * 0.94) * STEP, dn = Math.log10(Math.max(cube(dcy), 1e-9) * 0.94) * 0.25 * STEP;
  while (stage < 2 && n < MAX) {
    if (stage === 0) { v += up; if (v >= 1) { v = 1; stage = 1; } }
    else if (v < sus) { v = sus; stage = 2; }
    else { v += dn * ((0.01 + v - sus) / (1 - sus)) ** slope; if (v <= sus) { v = sus; stage = 2; } v = Math.min(v, 1); if (dn >= 0) stage = 2; }
    n++; pts.push([n * B, v]);
  }
  const off = n * B + Math.max(0.05, 0.25 * n * B); pts.push([off, v]);
  const rel = Math.log10(Math.max(cube(r), 1e-9) * 0.94) * 0.25 * STEP;
  for (let k = 1; v > 1e-5 && k < MAX; k++) { v += rel * Math.max(v, 0) ** slope; if (rel >= 0) break; pts.push([off + k * B, Math.max(v, 0)]); }
  return { pts, off };
}
// Tunefish-mode stage times in seconds (same integration as envCurve): attack 0 → 1, decay 1 → sustain (with the
// current sustain and slope), release from full level to silence. Infinity when the stage never ends.
function tfEnvSeconds(stage, x, s, slope) {
  const B = 512 / 44100, STEP = 0.0005 * 512, cube = q => q * q * q, MAX = 600 / B;
  if (stage === "attack") return B * (1 / (-Math.log10(Math.max(cube(x), 1e-9) * 0.94) * STEP));
  const rate = Math.log10(Math.max(cube(x), 1e-9) * 0.94) * 0.25 * STEP; if (rate >= 0) return Infinity;
  let v = 1, n = 0;
  if (stage === "decay") { const sus = Math.min(s, 0.99); while (v > sus && n < MAX) { v += rate * ((0.01 + v - sus) / (1 - sus)) ** slope; n++; } }
  else while (v > 1e-5 && n < MAX) { v += rate * Math.max(v, 0) ** slope; n++; }
  return n >= MAX ? Infinity : n * B;
}
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
  };
  return svg;
}
// Sampler slot (unisynth smp_source): `osc<i>.s7` picks the source — choice of 3: GSM sample of the device (0), GM.DLS
// instrument (0.5; a region per note: index = 1 + drums·16384 + bank·128 + program) or one GM.DLS wave (1.0; index =
// wave + 1) — and `osc<i>.sample` holds the raw index (0 = none). Both are structural (the engine rebuilds the device).
const NOBANK = new URLSearchParams(location.search).has("nobank");
const SMP_SOURCES = ["GSM sample", "GM instrument (per-note regions)", "GM wave"];
const gmInstrument = idx => { if (idx < 1) return null; const n = idx - 1, drums = n >= 16384, r = n % 16384; return { drums, bank: Math.floor(r / 128), prog: r % 128 }; };
function samplerPre(t, d, i) {
  const box = document.createElement("div"); box.className = "sampler";
  const src = choice(vget(d, ip(`osc${i}.s7`)), 3), idx = Math.round(vget(d, ip(`osc${i}.sample`))), nGsm = d.samples?.length ?? 0;
  let what;
  if (src === 0) { const kb = idx && idx <= nGsm ? ` · ${(d.samples[idx - 1].length * 3 / 4 / 1024).toFixed(1)} KB` : "";
    what = idx ? `GSM sample ${idx} of ${nGsm} in this instrument${idx > nGsm ? " (missing: plays silence)" : kb}` : `no GSM sample chosen (${nGsm} in this instrument)`; }
  else if (src === 1) { const k = gmInstrument(idx), g = k && CAT?.gm?.instruments.find(x => !!x.drums === k.drums && x.bank === k.bank && x.prog === k.prog);
    what = k ? `GM.DLS ${k.drums ? "drum kit" : "instrument"} ${g ? `“${g.name}”` : ""} bank ${k.bank} program ${k.prog}${g ? ` · ${g.regions.length} regions` : CAT?.gm ? " (not in this bank)" : ""}` : "no GM instrument chosen"; }
  else { const g = CAT?.gm?.instruments.find(x => x.regions.some(r => r[2] === idx - 1)); what = idx ? `GM.DLS wave ${idx - 1}${g ? ` · from ${g.name}` : ""}` : "no wave chosen"; }
  box.innerHTML = `<div class="srow"><label class="pchoice"><span>source</span><select aria-label="Osc ${i} sample source">${SMP_SOURCES.map((x, k) => `<option value="${k}"${k === src ? " selected" : ""}>${x}</option>`).join("")}</select></label>
    <span class="dim">${what}</span>${src !== 0 ? `<button class="gmpick">Choose GM instrument…</button>` : `${idx && idx <= nGsm ? `<button class="gsmplay" aria-label="Play GSM sample ${idx}">▶ Play</button>` : ""}<button class="gsmpick">Choose GSM sample…</button><button class="gsmimport">Import WAV…</button>${nGsm > gsmUsed(d).size ? `<button class="gsmclean" title="Samples no slot plays still cost bytes in the intro">Remove unused (${nGsm - [...gsmUsed(d)].filter(k => k >= 1 && k <= nGsm).length})</button>` : ""}`}</div>
    ${src !== 0 ? (!CAT?.gm || NOBANK ? `<div class="warn-note" role="alert">GM bank not found: set GM_DLS or install the OS bank. The song keeps the index and plays once a bank is present.</div>`
      : `<div class="info-note">Uses the OS bank at run time (Windows gm.dls, macOS gs_instruments.dls): sounds differ per platform.${src === 2 ? " One wave plays over all keys; the GM instrument source picks a region per note." : ""}</div>`) : ""}`;
  box.querySelector("select").addEventListener("change", e => {
    const before = snap(), k = +e.target.value; d.values[`osc${i}.s7`] = k / 2;
    liveParam(t, d, ip(`osc${i}.s7`), k / 2); renderDevice(); commit(before, `${t.name}: osc ${i} source ${SMP_SOURCES[k]}`);
  });
  box.querySelector(".gmpick")?.addEventListener("click", () => openLibrary("gm"));
  box.querySelector(".gsmpick")?.addEventListener("click", () => openLibrary("gsm"));
  box.querySelector(".gsmimport")?.addEventListener("click", () => importWav(t, d, i));
  box.querySelector(".gsmplay")?.addEventListener("click", () => playSample(d.samples[idx - 1]));
  box.querySelector(".gsmclean")?.addEventListener("click", () => removeUnusedGsm(t, d));
  if (src === 0 ? idx >= 1 && idx <= nGsm : idx >= 1) samplerSettings(t, d, i, box, src === 0 ? d.samples[idx - 1] : null);
  return box;
}
// Playback settings of a sampler slot (shape values s0–s6, crates/unisynth/src/smp.rs) as named controls, plus the
// sample's waveform with its start and loop: drag the start line, the loop edges, or the loop body to move it.
const LOOP_MODES = ["Off", "Repeat", "Ping-pong"], LOOP_REGIONS = ["Sample's own loop", "Custom"], INTERPS = ["Nearest", "Linear"];
const pcmCache = new Map();
const pcmOf = b64 => { if (!pcmCache.has(b64)) pcmCache.set(b64, E.gsmDecode(b64ToBytes(b64))); return pcmCache.get(b64); };
function samplerSettings(t, d, i, box, b64) {
  const S = k => `osc${i}.s${k}`, cv = k => curVal(d, S(k));
  const sel3 = (label, k, opts) => `<label class="pchoice"><span>${label}</span><select data-s="${k}" aria-label="Osc ${i} ${label}">${opts.map((o, j) => `<option value="${j}"${j === choice(vget(d, ip(S(k))), opts.length) ? " selected" : ""}>${o}</option>`).join("")}</select></label>`;
  const row = document.createElement("div"); row.className = "srow smpset";
  row.innerHTML = sel3("loop", 1, LOOP_MODES) + (choice(vget(d, ip(S(1))), 3) ? sel3("region", 2, LOOP_REGIONS) : "") + sel3("interp", 6, INTERPS) +
    `<label class="pchoice"><input type="checkbox" data-s="5"${choice(vget(d, ip(S(5))), 2) ? " checked" : ""} aria-label="Osc ${i} reverse"><span>reverse</span></label>`;
  row.querySelectorAll("[data-s]").forEach(el => el.addEventListener("change", () => {
    const k = +el.dataset.s, n = k === 1 ? 3 : 2, j = el.type === "checkbox" ? +el.checked : +el.value, before = snap();
    d.values[S(k)] = j / (n - 1); liveParam(t, d, ip(S(k)), j / (n - 1)); renderDevice();
    commit(before, `${t.name}: osc ${i} ${["", "loop", "loop region", "", "", "reverse", "interpolation"][k]} ${el.type === "checkbox" ? (j ? "on" : "off") : el.selectedOptions[0].text}`);
  }));
  const pcm = b64 ? pcmOf(b64) : null, W = 360, H = 52;
  const cv2 = document.createElement("canvas"); cv2.className = "smpwave"; cv2.width = W * devicePixelRatio; cv2.height = H * devicePixelRatio; cv2.style.width = W + "px"; cv2.style.height = H + "px";
  cv2.setAttribute("role", "img");
  const peaks = new Float32Array(W * 2);
  if (pcm?.length) for (let x = 0; x < W; x++) { let lo = 0, hi = 0; const a = Math.floor(x * pcm.length / W), b = Math.max(a + 1, Math.floor((x + 1) * pcm.length / W)); for (let k = a; k < b; k++) { lo = Math.min(lo, pcm[k]); hi = Math.max(hi, pcm[k]); } peaks[2 * x] = lo; peaks[2 * x + 1] = hi; }
  let pk = 0; for (const x of peaks) pk = Math.max(pk, Math.abs(x)); const gainV = pk > 1e-4 ? 0.95 / pk : 1;  // drawn normalised
  const loopOf = () => { const on = choice(cv(1), 3), custom = choice(cv(2), 2); if (!on) return null; if (!custom) return pcm ? [0, 1] : null; const s0 = cv(3); return [s0, Math.min(1, s0 + cv(4))]; };
  const draw = () => {
    const g = cv2.getContext("2d"); g.setTransform(devicePixelRatio, 0, 0, devicePixelRatio, 0, 0); g.clearRect(0, 0, W, H);
    g.fillStyle = css("--bg"); g.fillRect(0, 0, W, H);
    const lp = loopOf(), rev = choice(cv(5), 2), st = cv(0), sx = (rev ? 1 - st : st) * W;
    if (lp) { g.fillStyle = css("--focus") + "33"; g.fillRect(lp[0] * W, 0, (lp[1] - lp[0]) * W, H); g.fillStyle = css("--focus"); g.fillRect(lp[0] * W, 0, 2, H); g.fillRect(lp[1] * W - 2, 0, 2, H); }
    g.fillStyle = css("--fg") + "aa";
    if (pcm?.length) for (let x = 0; x < W; x++) { const a = H / 2 - peaks[2 * x + 1] * gainV * H / 2, b = H / 2 - peaks[2 * x] * gainV * H / 2; g.fillRect(x, a, 1, Math.max(1, b - a)); }
    else { g.fillStyle = css("--dim"); g.font = "10px sans-serif"; g.fillText("GM bank sample: waveform not shown; positions are fractions of it", 6, H / 2 + 3); }
    g.strokeStyle = css("--ok") || "#4c4"; g.lineWidth = 2; g.beginPath(); g.moveTo(sx, 0); g.lineTo(sx, H); g.stroke();
    g.fillStyle = css("--ok") || "#4c4"; g.beginPath(); g.moveTo(sx, 0); g.lineTo(sx + (rev ? -7 : 7), 0); g.lineTo(sx, 7); g.fill();
    const secs = pcm ? pcm.length / 44100 : 0;
    cv2.setAttribute("aria-label", `Osc ${i} sample${secs ? ` ${secs.toFixed(2)} s` : ""}: start ${(st * 100).toFixed(1)} %${rev ? " (reverse)" : ""}${lp ? `, loop ${(lp[0] * 100).toFixed(1)}–${(lp[1] * 100).toFixed(1)} %` : ", no loop"}`);
    cv2.title = cv2.getAttribute("aria-label") + " · drag the start line or the loop";
  };
  // writes a shape value: song, engine, the knob showing it (if any)
  const put = (k, v) => { v = Math.min(1, Math.max(0, v)); d.values[S(k)] = v; liveParam(t, d, ip(S(k)), v); const el = knobReg.get(d.values)?.get(S(k)); if (el?.isConnected) el._set(v); };
  cv2.addEventListener("pointerdown", e => {
    const r = cv2.getBoundingClientRect(), f = x => (x - r.left) / r.width, f0 = f(e.clientX), near = (a, px = 5) => Math.abs((f0 - a) * r.width) <= px;
    const lp = loopOf(), rev = choice(cv(5), 2), st = cv(0), sPos = rev ? 1 - st : st, custom = choice(cv(2), 2);
    let what = near(sPos) ? "start" : lp && near(lp[0]) ? "ls" : lp && near(lp[1]) ? "le" : lp && custom && f0 > lp[0] && f0 < lp[1] ? "move" : "start";
    if (!custom && what !== "start") { put(2, 1); put(3, lp[0]); put(4, lp[1] - lp[0]); }   // dragging the sample's own loop makes it custom
    const before = snap(), l0 = cv(3), len0 = cv(4); cv2.setPointerCapture(e.pointerId);
    const mv = ev => {
      const x = Math.min(1, Math.max(0, f(ev.clientX)));
      if (what === "start") put(0, rev ? 1 - x : x);
      else if (what === "ls") { const end = l0 + len0; put(3, Math.min(x, end - 0.001)); put(4, end - cv(3)); }
      else if (what === "le") put(4, Math.max(0.001, x - l0));
      else { const nl = Math.min(1 - len0, Math.max(0, l0 + x - f0)); put(3, nl); }
      draw();
    };
    mv(e); cv2.addEventListener("pointermove", mv);
    cv2.addEventListener("pointerup", () => { cv2.removeEventListener("pointermove", mv); const lbl = { start: "start", ls: "loop start", le: "loop end", move: "loop" }[what];
      d._edited = true; commit(before, `${t.name}: osc ${i} sample ${lbl} moved`); if (!custom && what !== "start") renderDevice(); }, { once: true });
  });
  const wrap = document.createElement("div"); wrap.className = "smpwrap";
  const ov = document.createElementNS(SVGNS, "svg"); ov.setAttribute("class", "smplive"); ov.setAttribute("viewBox", "0 0 100 40");
  ov.setAttribute("preserveAspectRatio", "none"); ov.setAttribute("aria-hidden", "true"); ov.dataset.live = `smp${i}`;
  wrap.append(cv2, ov);
  box.append(row, wrap); box._draw = draw; draw();
}
// ---- GSM samples: stored (base64) in the instrument's `samples`; a sampler slot plays one by 1-based number. From the
// library (samples in project songs), or imported from an audio file: decoded and resampled to 44.1 kHz by the
// browser, mixed to mono, encoded to GSM 06.10 by the engine (bit-exact libgsm port, about 9 KB per second). ----
const b64ToBytes = s => Uint8Array.from(atob(s), c => c.charCodeAt(0));
const bytesToB64 = b => { let s = ""; for (let i = 0; i < b.length; i += 0x8000) s += String.fromCharCode(...b.subarray(i, i + 0x8000)); return btoa(s); };
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
function instrumentBox(t, d) {
  const box = document.createElement("div"); box.className = "devbox"; box.setAttribute("aria-label", "Instrument");
  box.innerHTML = `<h3>Unisynth <span class="kind">kind 64 · preset ${d.preset || "init"}${d._edited ? "*" : ""}</span><span class="vox" role="group" aria-label="Sounding voices"></span>
    <span class="acts"><button class="livev" aria-pressed="${liveOn()}" title="Live view: show the sounding voices on the screens (costs some engine time while on)">Live</button><button class="browse">Browse…</button><button class="impsmp" title="Encode an audio file (WAV, AIFF, MP3…) as a GSM sample and play it on a sampler slot (a free oscillator if there is no GSM sampler)">Import sample…</button><button class="savepre">Save preset</button><button class="bypass" aria-pressed="${!!d.bypass}" title="Bypass the instrument" aria-label="Bypass instrument">⏻</button></span></h3>`;
  box.classList.toggle("byp", !!d.bypass);
  box.querySelector(".browse").onclick = () => openLibrary();
  box.querySelector(".impsmp").onclick = () => importWav(t, d, gsmSlot(d));
  box.querySelector(".bypass").onclick = () => devBypass(t, t.devices.indexOf(d));
  box.querySelector(".savepre").onclick = e => savePreset(t, d, e.currentTarget);
  box.querySelector(".livev").onclick = e => { store.set("daw.live", liveOn() ? "0" : "1"); e.currentTarget.setAttribute("aria-pressed", String(liveOn())); updateWatch(); };
  const body = document.createElement("div"); body.className = "devbody";
  const P = names => names.map(ip);
  const oscCard = i => {
    const ty = oscType(d, i), shapes = OSC_SHAPES[ty] || [];
    const labels = Object.fromEntries(shapes.map((nm, k) => [`osc${i}.s${k}`, nm]));
    const ks = ty === 4 ? smpShapes(d, i) : shapes.map((_, k) => k);
    const ps = [ip(`osc${i}.type`), ...OSC_COMMON.map(f => ip(`osc${i}.${f}`)), ...ks.map(k => ip(`osc${i}.s${k}`)), ...(ty === 4 ? [ip(`osc${i}.sample`)] : [])];
    if (ty === 4) labels[`osc${i}.sample`] = "sample #";
    return slotCard(t, d, "osc", i, pgroup(t, d, `Osc ${i}`, ps, { cols: 6, labels, pre: ty === 4 ? samplerPre(t, d, i) : ty ? oscView(d, i) : null }));
  };
  const range = n => [...Array(n)].map((_, k) => k + 1);
  const vc = col(
    pgroup(t, d, "Voice", P(["mode", "steal", "one_shot", "env1_at_output", "master", "pan", "capacity", "unison", "uni_detune", "uni_spread", "slide", "glide", "transpose", "velocity", "start_spread", "noise_level"]), { cols: 4 }),
    pgroup(t, d, "Noise", P(["noise.band_on", "noise.hp", "noise.lp"]), { cols: 2 }));
  vc.dataset.k = "voice"; vc.classList.add("c-voice"); body.append(vc);
  body.append(col(slotBlock("osc", "Oscillators", usedCount(d, "osc"), range(3).map(oscCard))));
  body.append(col(slotBlock("filt", "Filters · serial", usedCount(d, "filt"),
    filterItems(t, d, i => slotCard(t, d, "filt", i, pgroup(t, d, `Filter ${i}`, P([`filt${i}.type`, `filt${i}.cutoff`, `filt${i}.resonance`]), { cols: 2, pre: filtView(t, d, i) }))))));
  body.append(col(slotBlock("env", "Envelopes", usedCount(d, "env"), range(4).map(i =>
    slotCard(t, d, "env", i, pgroup(t, d, `Env ${i}`, P([`env${i}.law`, `env${i}.attack`, `env${i}.decay`, `env${i}.sustain`, `env${i}.release`, `env${i}.curve_slope`]), { cols: 3, pre: envView(d, i) }))))));
  body.append(col(
    slotBlock("lfo", "LFOs", usedCount(d, "lfo"), range(2).map(i => slotCard(t, d, "lfo", i, pgroup(t, d, `LFO ${i}`, P([`lfo${i}.shape`, `lfo${i}.sync`, `lfo${i}.rate`, `lfo${i}.depth`]), { cols: 2, pre: lfoView(d, i) })))),
    slotBlock("mod", "Mod matrix", usedCount(d, "mod"), [modTable(t, d)]),
    status));
  box.append(body); return box;
}
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
const effectChoices = () => TABLES.filter(tb => tb.kind !== 64);
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
    const before = snap(); d.preset = nm; d._edited = false; close(); CAT = await api.library() || CAT; lib.setCatalog(CAT); renderAll(); commit(before, `saved preset ${nm} → ${path}`);
  };
  pop.addEventListener("keydown", e => { if (e.key === "Escape") close(); if (e.key === "Enter") pop.querySelector(".ok").click(); });
}
function fxBox(t, d, i) {
  const tb = tableOf(d), name = devName(d), box = document.createElement("div"); box.className = "devbox"; box.setAttribute("aria-label", `Effect ${name}`);
  const fx = t.devices.map((x, k) => [x, k]).filter(([x]) => x.kind !== 64).map(e => e[1]), pos = fx.indexOf(i);
  const side = tracks.filter(x => x.sends.some(s1 => s1[0] === t.name && s1[1] === "side"));
  box.innerHTML = `<h3>${tableOf(d) ? (d.kind === 64 ? name : (tb.label.split(" · ")[0])) : name} <span class="kind">kind ${d.kind}${side.length && d.kind === 72 ? " · sidechain ← " + side.map(x => x.name).join(", ") : ""}</span><span class="acts"><button class="mvl" title="Move earlier in the chain" aria-label="Move ${name} earlier" ${pos <= 0 ? "disabled" : ""}>◀</button><button class="mvr" title="Move later in the chain" aria-label="Move ${name} later" ${pos === fx.length - 1 ? "disabled" : ""}>▶</button><button class="bypass" aria-pressed="${!!d.bypass}" title="Bypass" aria-label="Bypass ${name}">⏻</button><button title="Remove" aria-label="Remove ${name}">✕</button></span></h3>`;
  box.classList.toggle("byp", !!d.bypass);
  box.querySelector(".bypass").onclick = () => devBypass(t, i);
  box.querySelector(".mvl").onclick = () => moveDev(t, i, -1); box.querySelector(".mvr").onclick = () => moveDev(t, i, 1);
  box.querySelector('[title="Remove"]').onclick = () => removeDev(t, i);
  const body = document.createElement("div"); body.className = "devbody";
  if (tb) {
    const ps = tb.params.filter(p => p.name !== "mode" || nModes(d.kind) > 1);
    const g = pgroup(t, d, tb.label, ps, { cols: Math.min(3, ps.length) });
    // a mode change selects another parameter set: keep only the mode
    g.querySelector(`select[aria-label$=" mode"]`)?.addEventListener("change", () => { for (const k of Object.keys(d.values)) if (k !== "mode") delete d.values[k]; renderDevice(); livePush(); });
    body.append(g);
  } else body.append(Object.assign(document.createElement("div"), { className: "warn-note", textContent: `kind ${d.kind}: no parameter table` }));
  box.append(body); return box;
}
const device = document.getElementById("device");
function renderDevice() {
  const t = selected, chain = document.createElement("div"); chain.className = "chain";
  const inst = instOf(t);
  if (inst) chain.append(instrumentBox(t, inst));
  else chain.append(Object.assign(document.createElement("div"), { className: "noinst", innerHTML: `<b>${t.master ? "Master" : "Bus"}</b><span class="dim">${t.master ? "end of the graph → song output" : "no instrument · fed by sends from " + (senders(t).map(x => x.name).join(", ") || "nothing")}</span>` }));
  t.devices.forEach((d, i) => { if (d.kind !== 64) chain.append(fxBox(t, d, i)); });
  const add = document.createElement("button"); add.className = "addfx"; add.textContent = "+ Effect";
  add.onclick = () => { const pick = document.createElement("select"); pick.setAttribute("aria-label", `Effect to add to ${t.name}`);
    pick.innerHTML = `<option value="">choose effect…</option>` + effectChoices().map(tb => `<option>${tb.label}</option>`).join(""); add.replaceWith(pick); pick.focus();
    pick.onchange = () => { if (pick.value) addEffect(t, pick.value); }; pick.onblur = () => { if (!pick.value) pick.replaceWith(add); }; };
  chain.append(add);
  device.replaceChildren(chain);
  const clip = t.clips?.find(c => c.sel) || t.clips?.[0];
  document.getElementById("ctx").innerHTML = `<span class="dim">Track</span> <b style="color:var(${t.col})">${t.name}</b>` +
    (clip ? ` <span class="dim">· clip</span> <b>${clip.name}</b>${links(t, clip.name) > 1 ? ` <span class="badge">⧉ ${links(t, clip.name)} linked</span>` : ""}` : "") +
    ` <span class="dim">· ${t.devices.length} device${t.devices.length === 1 ? "" : "s"}</span>`;
  applyAutomation(); updateReauto(t);
  updateWatch(); liveDirty = true;
}

// ---- Live per-voice displays (change add-device-live-view D7): the engine's probe of the watched (selected) track's
// instrument arrives with each ~30 ms status; an animation frame redraws only the overlay groups (`g.live`). Each
// voice keeps one colour (by its note-on id) in every screen and a chip in the header; at most 16 voices are drawn,
// the loudest; markers at one spot stack. Filters: input spectrum grey, output in the block colour, a tick per voice
// at its modulated cutoff. ----
const VCOL = ["#ffd166", "#4cc9f0", "#f72585", "#80ed99", "#ff8c42", "#b8a1ff", "#f4f1de", "#00f5d4", "#ff70a6", "#70d6ff", "#e9ff70", "#ff9770", "#c77dff", "#90f1ef", "#ffd6e0", "#a3e635"];
const MAX_DRAWN = 16, VREC = 22;
const vcol = new Map();  // voice id → palette index
const relAt = new Map(); // voice id → age at which it was first seen released
let liveP = null;         // { voices, taps, at } from the last probe
function liveOn() { return store.get("daw.live") !== "0"; }  // live view on/off, remembered in the browser
function updateWatch() {
  const t = selected, on = liveOn() && !document.getElementById("p-dev").hidden && t && !t.bus && !t.master && instOf(t);
  E.watch(on ? trackIndex(t) : -1);
  if (!on) { liveP = null; liveDirty = true; }
}
function parseProbe(p) {
  if (!p || p[0] !== 1) return null;
  const nv = p[1], nf = p[2], L = p[3], voices = [], taps = [];
  if (p.length < 4 + nv * VREC + nf) return null;  // layout mismatch (VREC = synth-wasm Host::probe's voice record)
  let o = 4;
  for (let v = 0; v < nv; v++, o += VREC) {
    const r = p.subarray(o, o + VREC);
    voices.push({ id: r[0], note: r[1], amp: r[2], rel: r[3] > 0, age: r[4], env: [0, 1, 2, 3].map(k => [r[5 + 2 * k], r[6 + 2 * k]]),
      lfo: [r[13], r[14]], smp: [r[15], r[16], r[17]], cut: [r[18], r[19], r[20], r[21]] });
  }
  for (let f = 0; f < nf && o < p.length; f++) { if (p[o++]) { if (o + 2 * L > p.length) return null; taps[f] = [p.subarray(o, o + L), p.subarray(o + L, o + 2 * L)]; o += 2 * L; } }
  return { voices, taps };
}
// 2048-point Hann FFT of the taps, folded onto the screens' log axis (x = 100·log10(f/20)/3), with a short peak hold
const FN = 2048, FBINS = 140, fwin = Float64Array.from({ length: FN }, (_, i) => .5 - .5 * Math.cos(2 * Math.PI * i / (FN - 1)));
const frev = Uint16Array.from({ length: FN }, (_, i) => { let r = 0; for (let b = 0, x = i; b < 11; b++, x >>= 1) r = (r << 1) | (x & 1); return r; });
const fre = new Float64Array(FN), fim = new Float64Array(FN);
function spectrumDb(x) {
  for (let i = 0; i < FN; i++) { fre[frev[i]] = x[i] * fwin[i]; fim[frev[i]] = 0; }
  for (let len = 2; len <= FN; len <<= 1) {
    const ang = -2 * Math.PI / len, wr = Math.cos(ang), wi = Math.sin(ang);
    for (let i = 0; i < FN; i += len) { let cr = 1, ci = 0;
      for (let k = 0; k < len / 2; k++) { const a = i + k, b = a + len / 2, tr = fre[b] * cr - fim[b] * ci, ti = fre[b] * ci + fim[b] * cr;
        fre[b] = fre[a] - tr; fim[b] = fim[a] - ti; fre[a] += tr; fim[a] += ti; const nr = cr * wr - ci * wi; ci = cr * wi + ci * wr; cr = nr; } }
  }
  const out = new Float64Array(FBINS + 1), rate = E.rate || 44100;
  for (let b = 0; b <= FBINS; b++) {
    const f0 = 20 * 10 ** ((b - .5) / FBINS * 3), f1 = 20 * 10 ** ((b + .5) / FBINS * 3);
    const k0 = Math.max(1, Math.floor(f0 / rate * FN)), k1 = Math.min(FN / 2 - 1, Math.max(k0, Math.ceil(f1 / rate * FN)));
    let m = 0; for (let k = k0; k <= k1; k++) m = Math.max(m, Math.hypot(fre[k], fim[k]));
    out[b] = 20 * Math.log10(m / (FN / 4) + 1e-6);
  }
  return out;
}
const specHold = new Map();  // `${slot}:in|out` → dB bins with peak hold
function holdSpec(key, db) { const h = specHold.get(key); if (h) for (let b = 0; b < db.length; b++) db[b] = Math.max(db[b], h[b] - 2.5); specHold.set(key, db); return db; }
let liveDirty = false;
const liveCost = { probe: 0, nProbe: 0, draw: 0, nDraw: 0, voices: 0 };  // ms spent (DAW.debug.live)
function liveProbe(p) {
  const t0 = performance.now();
  try { liveProbe1(p); } finally { liveCost.probe += performance.now() - t0; liveCost.nProbe++; }
}
function liveProbe1(p) {
  const pr = liveOn() ? parseProbe(p) : null;  // a status sent before the view went off
  if (!pr && !liveP) return;
  liveP = pr; liveDirty = true;
  if (!pr) return;
  const ids = new Set(pr.voices.map(v => v.id));
  for (const id of [...vcol.keys()]) if (!ids.has(id)) { vcol.delete(id); relAt.delete(id); }
  for (const v of pr.voices) {
    // a free palette entry; with all 16 taken (more voices than are drawn) the voice shares one, starting at its id
    if (!vcol.has(v.id)) { const used = new Set(vcol.values()); let c = 0; while (used.has(c) && c < VCOL.length) c++; vcol.set(v.id, c < VCOL.length ? c : v.id % VCOL.length); }
    if (v.rel && !relAt.has(v.id)) relAt.set(v.id, v.age);
  }
  pr.spec = pr.taps.map((t, f) => t && [holdSpec(`${f}:in`, spectrumDb(t[0])), holdSpec(`${f}:out`, spectrumDb(t[1]))]);
}
const vmark = (x, y, c, a, w = 6) => `<line class="vm" x1="${x.toFixed(2)}" y1="${y.toFixed(2)}" x2="${x.toFixed(2)}" y2="${y.toFixed(2)}" stroke="${c}" stroke-width="${w}" stroke-opacity="${a.toFixed(2)}"/>`;
function beads(h) { const seen = []; return (x, y, c, a, dy, top) => { const k = seen.filter(b => Math.abs(b[0] - x) < dy * .6 && Math.abs(b[1] - y) < dy).length; seen.push([x, y]); return h(x, y + (y < top ? 1 : -1) * k * dy, c, a); }; }
function drawLive() {
  if (!liveDirty) return; liveDirty = false;
  const t0 = performance.now();
  try { drawLive1(); } finally { liveCost.draw += performance.now() - t0; liveCost.nDraw++; liveCost.voices = Math.max(liveCost.voices, liveP?.voices.length || 0); }
}
function drawLive1() {
  const root = document.getElementById("device");
  root.querySelectorAll("g.live").forEach(g => g.remove());
  const vox = root.querySelector(".vox");
  const vs = (liveP?.voices || []).slice().sort((a, b) => b.amp - a.amp), shown = vs.slice(0, MAX_DRAWN);
  if (vox) vox.innerHTML = shown.map(v => `<i style="--vc:${VCOL[vcol.get(v.id)]}" class="${v.rel ? "rel" : ""}">${NAMES[v.note % 12]}${Math.floor(v.note / 12) - 1}</i>`).join("") + (vs.length > MAX_DRAWN ? `<b>+${vs.length - MAX_DRAWN}</b>` : "");
  if (!shown.length) return;
  const d = instOf(selected), alpha = v => .25 + .75 * Math.min(1, v.amp / Math.max(1e-6, shown[0].amp));
  for (const svg of root.querySelectorAll("svg[data-live]")) {
    if (!svg.isConnected || svg.closest(".closed")) continue;
    const m = svg.dataset.live.match(/^([a-z]+)(\d)$/), g = m[1], i = +m[2];
    let h = "";
    if (g === "smp") for (const v of shown) { const pos = v.smp[i - 1]; if (pos < 0) continue; const x = Math.min(100, pos * 100), c = VCOL[vcol.get(v.id)];
      h += `<line class="vm" x1="${x}" y1="0" x2="${x}" y2="40" stroke="${c}" stroke-width="1.5" stroke-opacity="${alpha(v).toFixed(2)}"/>`; }
    if (g === "env" && svg._geo) { const { x, y, off, tot } = svg._geo, bead = beads(vmark), vb = svg.viewBox.baseVal;
      shown.forEach((v, k) => { const [lvl] = v.env[i - 1], ra = relAt.get(v.id), t = ra == null ? Math.min(v.age, off) : Math.min(tot, off + (v.age - ra)), c = VCOL[vcol.get(v.id)];
        h += bead(x(t), y(lvl), c, v.rel ? .55 : 1, 5, vb.height / 2) + `<line class="vm" x1="${vb.width - 3 - (k % 4) * 1.5}" y1="${y(0)}" x2="${vb.width - 3 - (k % 4) * 1.5}" y2="${y(lvl)}" stroke="${c}" stroke-width="1.5" stroke-opacity=".8"/>`; }); }
    if (g === "lfo" && d) { const f = LFO_SHAPES[choice(curVal(d, `lfo${i}.shape`), 5)], dep = .25 + .75 * curVal(d, `lfo${i}.depth`), bead = beads(vmark);
      for (const v of shown) { const ph = ((v.lfo[i - 1] % 1) + 1) % 1; h += bead(ph * 100, 20 - 15 * dep * f(ph), VCOL[vcol.get(v.id)], alpha(v), 4.5, 20); } }
    if (g === "filt") { const sp = liveP.spec?.[i - 1];
      if (sp) { const path = db => "M" + Array.from(db, (v, b) => `${(b / FBINS * 100).toFixed(2)} ${yDb(v).toFixed(2)}`).join(" L");
        const pin = path(sp[0]); h += `<path class="sin" d="${pin} L100 40 L0 40 Z"/><path class="sinl" d="${pin}"/><path class="sout" d="${path(sp[1])}"/>`; }
      for (const v of shown) { const hz = v.cut[i - 1]; if (hz < 0) continue; const x = Math.max(0, Math.min(100, Math.log10(Math.max(hz, 20) / 20) / .03));
        h += `<line class="vm ct" x1="${x.toFixed(2)}" y1="0" x2="${x.toFixed(2)}" y2="5" stroke="${VCOL[vcol.get(v.id)]}" stroke-width="2" stroke-opacity="${alpha(v).toFixed(2)}"/>`; } }
    if (h) svg.insertAdjacentHTML("beforeend", `<g class="live">${h}</g>`);
  }
  svg_playing(root);
}
// static curves step back while spectra are drawn
function svg_playing(root) { root.querySelectorAll("svg.scr[data-live^=filt]").forEach(s => s.classList.toggle("playing", !!s.querySelector("g.live path.sout"))); }
// Piano roll
const keys = document.getElementById("keys"), gridc = document.getElementById("gridc"), velc = document.getElementById("velc");
const NAMES = ["C", "C♯", "D", "D♯", "E", "F", "F♯", "G", "G♯", "A", "A♯", "B"];
(function liveFrames() { drawLive(); requestAnimationFrame(liveFrames); })();  // after NAMES (the live chips use it)
const LO = 24, HI = 96; let KH = 12, STEP = 22;  // zoomable: KH = key row height, STEP = px per 1/16
for (let n = HI; n >= LO; n--) {
  const b = document.createElement("button"); const nm = NAMES[n % 12];
  b.className = nm.includes("♯") ? "blk" : ""; b.textContent = nm === "C" ? "C" + (Math.floor(n / 12) - 1) : "";
  b.setAttribute("aria-label", `Audition ${nm}${Math.floor(n / 12) - 1}`); keys.append(b);
}
let rollNotes = [], rollClip = null, rollLen = 64;
let rollCC = { wheel: [], bend: [] };
function rollClipOf(t) {
  const r = rollClips.get(t.id), keep = r && (t.clips.includes(r) ? r : t.clips.find(c => c.name === r.name && c.start === r.start));
  const c = keep || t.clips.find(c => c.sel) || t.clips[0] || null; rollClips.set(t.id, c); return c;
}
function genRoll(t) {
  rollClip = t.bus || t.master ? null : rollClipOf(t);
  if (!rollClip) { rollNotes = []; rollCC = { wheel: [], bend: [] }; return; }
  const p = patOf(t, rollClip.name); rollNotes = p.notes; rollCC = p.cc; rollLen = Math.max(1, Math.round((rollClip.loop ? p.len : rollClip.len) * 16));
  const hdr = document.getElementById("roll-clip"); if (hdr) hdr.textContent = `${rollClip.name} · bar ${+rollClip.start.toFixed(2)} · ${+rollClip.len.toFixed(2)} bars${rollClip.loop ? ` · ⟳ ${p.len}-bar loop ×${+(rollClip.len / p.len).toFixed(2)}` : ""}${links(t, rollClip.name) > 1 ? ` · ⧉ ${links(t, rollClip.name)} linked` : ""}`;
}
const songStep = bar => (bar - rollClip.start) * 16;  // song bar -> step inside the open clip
// The roll and controller canvases cover only the visible part of the clip (a whole-song clip at full zoom would be
// tens of thousands of pixels wide: past the browser's canvas limit and hundreds of MB). `rollX0` = the clip x (px)
// at the canvas' left edge; drawing uses clip coordinates (translated), pointer positions add `rollX0`. Sizers keep
// the scroll width; scrolling redraws.
let rollX0 = 0, gridSize = null, velSize = null;
function rollView(W) {
  if (!gridSize) { gridSize = document.createElement("div"); velSize = document.createElement("div"); gridSize.className = velSize.className = "roll-size"; grid.prepend(gridSize); vel.prepend(velSize); }
  const vw = Math.max(1, Math.min(W, Math.ceil(grid.clientWidth || innerWidth)));  // hidden panel: a screen wide, never the clip
  rollX0 = Math.max(0, Math.min(Math.floor(grid.scrollLeft), W - vw));
  return vw;
}
let rollRedraw = 0;
const redrawRollSoon = () => { if (!rollRedraw) rollRedraw = requestAnimationFrame(() => { rollRedraw = 0; drawRoll(); }); };
function drawRoll() {
  if (!rollClip) { gridc.width = gridc.height = 0; velc.width = velc.height = 0; if (gridSize) gridSize.style.width = velSize.style.width = "0"; return; }
  const W = rollLen * STEP, H = (HI - LO + 1) * KH, dpr = devicePixelRatio || 1;
  const vw = rollView(W); gridSize.style.width = W + "px"; gridSize.style.height = H + "px";
  gridc.width = vw * dpr; gridc.height = H * dpr; gridc.style.width = vw + "px"; gridc.style.height = H + "px"; gridc.style.transform = `translateX(${rollX0}px)`;
  const g = gridc.getContext("2d"); g.scale(dpr, dpr); g.translate(-rollX0, 0);
  for (let n = HI; n >= LO; n--) { const y = (HI - n) * KH; g.fillStyle = NAMES[n % 12].includes("♯") ? css("--roll-black") : css("--panel"); g.fillRect(0, y, W, KH); }
  for (let s = 0; s <= rollLen; s++) { g.fillStyle = s % 16 === 0 ? css("--dim") : s % 4 === 0 ? css("--roll-beat") : css("--roll-step"); g.fillRect(s * STEP, 0, 1, H); }
  const l0 = Math.max(0, songStep(LOOP[0])), l1 = Math.min(rollLen, songStep(LOOP[1])); if (l1 > l0) { g.fillStyle = css("--loop"); g.fillRect(l0 * STEP, 0, (l1 - l0) * STEP, H); }
  for (const n of rollNotes) {
    const x = n.s * STEP + 1, y = (HI - n.n) * KH + 1;
    g.fillStyle = css(selected.col); g.globalAlpha = .45 + .55 * n.v; g.fillRect(x, y, n.l * STEP - 2, KH - 2); g.globalAlpha = 1;
    if (n.sel || n === velHover) { g.strokeStyle = n === velHover ? css("--focus") : css("--fg"); g.lineWidth = n === velHover ? 2 : 1.5; g.strokeRect(x + .5, y + .5, n.l * STEP - 3, KH - 3); }
  }
  document.getElementById("roll-ph").style.height = H + "px"; moveRollPh();
  g.strokeStyle = css("--fg"); g.setLineDash([3, 3]); g.beginPath(); g.moveTo(rCursor * STEP + .5, 0); g.lineTo(rCursor * STEP + .5, H); g.stroke(); g.setLineDash([]);
  if (rDrag?.mode === "marquee" && rDrag.moved) { const [a1, b1] = [Math.min(rDrag.st0, rDrag.st1), Math.max(rDrag.st0, rDrag.st1)], [k1, k2] = [Math.min(rDrag.key0, rDrag.key1), Math.max(rDrag.key0, rDrag.key1)];
    g.fillStyle = "#6aa7ff22"; g.strokeStyle = css("--focus"); g.fillRect(a1 * STEP, (HI - k2) * KH, (b1 - a1) * STEP, (k2 - k1 + 1) * KH); g.strokeRect(a1 * STEP + .5, (HI - k2) * KH + .5, (b1 - a1) * STEP, (k2 - k1 + 1) * KH); }
  const ns = rollNotes.filter(n => n.sel), vs = ns.map(n => Math.max(1, Math.round(n.v * 127))), vmin = Math.min(...vs), vmax = Math.max(...vs);
  document.getElementById("roll-info").textContent = ns.length ? `${ns.length} selected · ${NAMES[ns[0].n % 12]}${Math.floor(ns[0].n / 12) - 1} · vel ${vmin === vmax ? vmin : vmin + "–" + vmax}` : `${rollNotes.filter(n => n.s < rollLen).length} notes`;
  const vf = document.getElementById("vel-set"); if (document.activeElement !== vf) { vf.disabled = !ns.length; vf.value = ns.length && vmin === vmax ? vmin : ""; vf.placeholder = ns.length ? "mixed" : ""; }
  drawCC();
}
// Controller lane under the notes: velocity (per note, 1–127), mod wheel (0–127) and pitch bend (±range) as clip
// envelopes. Velocity: drag over stems paints, mouse wheel nudges (Shift ×8), selected notes move together.
// Wheel/bend: click adds, drag moves, double-click deletes (like automation lanes).
const VH = 84, ccKind = document.getElementById("cc-kind"), ccVal = document.getElementById("cc-val");
let ccTip = null, ccDrag = null, ccLast = null;
// the selected track's instrument bend range (unisynth ctl.bend_range, 0..24 semitones, default 2)
const bendRange = () => { const d = instOf(selected); return d ? +(vget(d, paramOf(d, "ctl.bend_range")) * 24).toFixed(2) : 2; };
const ccText = (k, v) => k === "vel" ? `vel ${Math.max(1, Math.round(v * 127))}` : k === "wheel" ? `MW ${Math.round(v * 127)}` : `PB ${((v - .5) * 2 * bendRange()).toFixed(2)} st`;
function drawCC() {
  const W = rollLen * STEP, dpr = devicePixelRatio || 1, k = ccKind.value, y = v => VH - 4 - v * (VH - 10);
  const vw = rollView(W); velSize.style.width = W + "px"; velSize.style.height = VH + "px";
  velc.width = vw * dpr; velc.height = VH * dpr; velc.style.width = vw + "px"; velc.style.height = VH + "px"; velc.style.transform = `translateX(${rollX0}px)`;
  const v = velc.getContext("2d"); v.scale(dpr, dpr); v.translate(-rollX0, 0);
  for (let s1 = 0; s1 <= rollLen; s1 += 4) { v.fillStyle = s1 % 16 === 0 ? css("--roll-beat") : css("--roll-step"); v.fillRect(s1 * STEP, 0, 1, VH); }
  if (k === "vel") {
    const sx = stems();
    for (const n of [...rollNotes.filter(x => !x.sel), ...rollNotes.filter(x => x.sel)]) { if (!sx.has(n)) continue; const x = sx.get(n);
      v.fillStyle = n === velHover ? css("--focus") : n.sel ? css("--fg") : css(selected.col); v.fillRect(x, y(n.v), 3, n.v * (VH - 10)); v.fillRect(x, y(n.v), 5, 2); }
  } else {
    const pts = rollCC[k], c = css(selected.col);
    if (k === "bend") { v.strokeStyle = css("--dim"); v.setLineDash([3, 3]); v.beginPath(); v.moveTo(0, y(.5)); v.lineTo(W, y(.5)); v.stroke(); v.setLineDash([]); }
    v.strokeStyle = c; v.lineWidth = 1.5; v.beginPath();
    const all = [[0, pts[0]?.[1] ?? (k === "bend" ? .5 : 0)], ...pts, [rollLen, pts.at(-1)?.[1] ?? (k === "bend" ? .5 : 0)]];
    all.forEach(([s1, val], i) => i ? v.lineTo(s1 * STEP, y(val)) : v.moveTo(s1 * STEP, y(val))); v.stroke();
    for (const [s1, val] of pts) { v.beginPath(); v.arc(s1 * STEP, y(val), 3.5, 0, 7); v.fillStyle = css("--bg"); v.fill(); v.stroke(); }
  }
  if (ccTip) { v.font = "10px ui-monospace, monospace"; const tw = v.measureText(ccTip.t).width + 8; v.fillStyle = css("--panel"); v.fillRect(Math.min(W - tw, ccTip.x + 8), 2, tw, 14); v.fillStyle = css("--fg"); v.fillText(ccTip.t, Math.min(W - tw, ccTip.x + 8) + 4, 12); }
}
const ccPos = e => { const r = velc.getBoundingClientRect(); return [(e.clientX - r.left + rollX0) / STEP, Math.min(1, Math.max(0, (r.top + VH - 4 - e.clientY) / (VH - 10)))]; };
// All notes whose stem is under the pointer (chords share a stem position).
// One stem per note. Notes that start together (chords) get stems 5 px apart, lowest pitch first, so each can be set.
let velHover = null;
function stems() {
  const m = new Map(), groups = new Map();
  for (const n of rollNotes) if (n.s < rollLen) { const k = Math.round(n.s * 8); (groups.get(k) || groups.set(k, []).get(k)).push(n); }
  for (const g of groups.values()) g.sort((a, b) => a.n - b.n).forEach((n, i) => m.set(n, n.s * STEP + 2 + i * 5));
  return m;
}
// The single note whose stem is nearest the pointer (within 4 px).
function stemAt(px) { let best = null, d = 4.5; for (const [n, x] of stems()) { const dd = Math.abs(px - (x + 1.5)); if (dd < d) { d = dd; best = n; } } return best; }
// Paint between two pointer positions so a fast sweep does not skip stems.
function paintSpan(px0, v0, px1, v1) {
  const [a, b] = px0 < px1 ? [px0, px1] : [px1, px0], hit = [];
  for (const [n, x] of stems()) { const c = x + 1.5; if (c >= a - 2 && c <= b + 2) { const f = b === a ? 1 : (c - px0) / (px1 - px0); n.v = clampV(v0 + (v1 - v0) * Math.min(1, Math.max(0, f))); hit.push(n); } }
  return hit;
}
const clampV = v => Math.min(1, Math.max(1 / 127, v));
const velTip = (e, ns) => { ccTip = { x: e.offsetX + rollX0, t: ccText("vel", ns[0].v) + (ns.length > 1 ? ` · ${ns.length} notes` : "") }; ccVal.textContent = ccTip.t; };
velc.addEventListener("pointerdown", e => {
  velc.setPointerCapture(e.pointerId); const [st, val] = ccPos(e), k = ccKind.value;
  if (k === "vel") {
    const n = stemAt(st * STEP), sel = selNotes(), before = snap(), solo = e.ctrlKey || e.metaKey;
    if (n && n.sel && sel.length > 1 && !solo) {      // group: every selected note by the same amount (Alt: all equal)
      ccDrag = { vel: "group", before, v0: val, orig: sel.map(x => [x, x.v]) };
      if (e.altKey) sel.forEach(x => x.v = clampV(val));
      velTip(e, sel);
    } else if (n) {                                   // one note: this stem only (Ctrl/⌘ inside a selection)
      ccDrag = { vel: "one", before, n }; n.v = clampV(val); velTip(e, [n]);
    } else {                                          // empty spot: sweep paints the stems it crosses
      ccDrag = { vel: "paint", before, px: st * STEP, v: val }; ccTip = null;
    }
    velHover = ccDrag.n || n; if (n) lastVel = n.v; drawRoll(); return;
  }
  const pts = rollCC[k], i = pts.findIndex(([s1, v1]) => Math.abs(s1 - st) < .4 && Math.abs(v1 - val) < .15), now = performance.now();
  if (i >= 0 && ccLast && ccLast.i === i && now - ccLast.t < 400) { pts.splice(i, 1); ccLast = null; drawCC(); ccVal.textContent = "point deleted"; return; }
  if (i >= 0) { ccDrag = { i }; ccLast = { i, t: now }; }
  else { const sn = Math.round(st * 4) / 4; pts.push([sn, val]); pts.sort((a1, b1) => a1[0] - b1[0]); ccDrag = { i: pts.findIndex(p1 => p1[0] === sn && p1[1] === val) }; }
  ccTip = { x: e.offsetX + rollX0, t: ccText(k, val) }; drawCC();
});
velc.addEventListener("pointermove", e => {
  if (!ccDrag) return; const [st, val] = ccPos(e), k = ccKind.value;
  if (ccDrag.vel === "group") { const d = val - ccDrag.v0; for (const [n, v0] of ccDrag.orig) n.v = clampV(e.altKey ? val : v0 + d); velTip(e, ccDrag.orig.map(o => o[0])); drawRoll(); return; }
  if (ccDrag.vel === "one") { ccDrag.n.v = clampV(val); velTip(e, [ccDrag.n]); lastVel = ccDrag.n.v; drawRoll(); return; }
  if (ccDrag.vel === "paint") { const hit = paintSpan(ccDrag.px, ccDrag.v, st * STEP, val); ccDrag.px = st * STEP; ccDrag.v = val; ccDrag.n_ = (ccDrag.n_ || 0) + hit.length;
    if (hit.length) { velTip(e, [hit.at(-1)]); velHover = hit.at(-1); lastVel = clampV(val); } drawRoll(); return; }
  const pts = rollCC[k], i = ccDrag.i, lo = i ? pts[i - 1][0] + .25 : 0, hi = i < pts.length - 1 ? pts[i + 1][0] - .25 : rollLen;
  pts[i] = [Math.min(hi, Math.max(lo, Math.round(st * 4) / 4)), e.altKey ? val : k === "bend" && Math.abs(val - .5) < .03 ? .5 : val];
  ccTip = { x: e.offsetX + rollX0, t: ccText(k, pts[i][1]) }; ccVal.textContent = ccTip.t; drawCC();
});
velc.addEventListener("pointermove", e => { if (ccDrag || ccKind.value !== "vel") return; const n = stemAt(ccPos(e)[0] * STEP); if (n !== velHover) { velHover = n; velc.style.cursor = n ? "ns-resize" : ""; drawRoll(); } });
velc.addEventListener("pointerleave", () => { if (!ccDrag && velHover) { velHover = null; drawRoll(); } });
velc.addEventListener("pointerup", () => { if (ccDrag?.vel) commit(ccDrag.before, ccDrag.vel === "group" ? `velocity of ${ccDrag.orig.length} notes changed` : ccDrag.vel === "one" ? `velocity ${Math.round(ccDrag.n.v * 127)} on ${NAMES[ccDrag.n.n % 12]}${Math.floor(ccDrag.n.n / 12) - 1}` : "velocity painted"); ccDrag = null; ccTip = null; drawRoll(); });
velc.addEventListener("wheel", e => {
  if (e.ctrlKey || e.metaKey) return; if (ccKind.value !== "vel") return; const h = stemAt(ccPos(e)[0] * STEP); if (!h) return; const hits = [h];
  e.preventDefault(); const step = (e.shiftKey ? 8 : 1) / 127 * (e.deltaY < 0 ? 1 : -1), before = snap();
  const ns = h.sel && !(e.ctrlKey || e.metaKey) ? selNotes() : hits; ns.forEach(n => n.v = clampV(n.v + step));
  commit(before, `velocity ${step > 0 ? "+" : "−"}${e.shiftKey ? 8 : 1} on ${ns.length} note${ns.length > 1 ? "s" : ""}`); ccVal.textContent = ccText("vel", ns[0].v); drawRoll();
}, { passive: false });
ccKind.addEventListener("change", () => { velc.setAttribute("aria-label", `Controller lane: ${ccKind.selectedOptions[0].text}`); ccVal.textContent = ""; drawCC(); });
// Audition controllers: MW holds, PB springs back to centre on release (live only, never recorded).
const pb = document.getElementById("pb"); pb.addEventListener("pointerup", () => { pb.value = 0; pb.dispatchEvent(new Event("input")); });
pb.addEventListener("keyup", () => { pb.value = 0; pb.dispatchEvent(new Event("input")); });
const grid = document.getElementById("grid"), vel = document.getElementById("vel");
grid.addEventListener("scroll", () => { keys.scrollTop = grid.scrollTop; vel.scrollLeft = grid.scrollLeft; if (Math.floor(grid.scrollLeft) !== rollX0 && rollClip) redrawRollSoon(); });
new ResizeObserver(redrawRollSoon).observe(grid);
vel.addEventListener("wheel", e => { if (!e.ctrlKey && !e.metaKey && !e.altKey && Math.abs(e.deltaX) > Math.abs(e.deltaY)) { e.preventDefault(); grid.scrollLeft += e.deltaX; } }, { passive: false });

// Mixer: each strip = inserts (the track's effect chain), sends (stored as receives on the target track,
// post-fader), fader, meters. Bus tracks have no instrument; the master is the last track and can hold effects.
const mixer = document.getElementById("mixer");
const FX = () => effectChoices().map(tb => tb.label);
const trackById = id => id === master.id ? master : tracks.find(x => x.id === id);
let sendBefore = null;  // undo snapshot of a send-level gesture
mountMixer(mixer, song, {
  select: id => selectTrack(trackById(id)),
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
function renderAll() {
  arrange.setLabelRev(v => v + 1);  // header device labels (fields like _edited are written outside the store)
  renderDevice();  // the mixer follows the song store by itself
}
function selectTrack(t) {
  t = (t.master ? master : tracks.find(x => x.id === t.id)) || t;  // always the store proxy (writes must notify)
  selected = t; setSelectedId(t.id);
  if (!t.bus && !t.master && instOf(t) && armed !== t) { armed = t; updateArmed(); }  // the keyboard plays the selected track
  genRoll(t); renderAll();
  if (!document.getElementById("p-roll").hidden) drawRoll();
}
document.getElementById("hdrs").addEventListener("click", e => {
  if (e.target.closest("button, input, select")) return;
  const h = e.target.closest(".thdr.track"); if (!h) return; const t = tracks.find(x => x.id === h.dataset.track);
  if (e.shiftKey || e.ctrlKey || e.metaKey) {  // Shift/Ctrl-click a header: add (or remove) all of its clips
    const all = t.clips.length && t.clips.every(c => c.sel); if (!e.shiftKey && !e.ctrlKey && !e.metaKey) clearClipSel();
    t.clips.forEach(c => c.sel = !all); tracks.forEach(renderLane); say(`${selClips().length} clips selected`);
  }
  selectTrack(t); focusArea = "arr";
});
document.getElementById("hdrs").addEventListener("keydown", e => {
  const h = e.target.closest(".thdr.track"); if (!h || e.target !== h) return;
  const i = tracks.findIndex(t => t.id === h.dataset.track);
  if (e.ctrlKey || e.metaKey || e.key === "Delete" || e.key === "Backspace" || e.key === "Escape") return arrKeys(e, tracks[i]);
  if (e.key === "Enter" || e.key === " ") { e.preventDefault(); selectTrack(tracks[i]); }
  if (e.key === "ArrowDown" || e.key === "ArrowUp") { e.preventDefault(); const n = tracks[Math.max(0, Math.min(tracks.length - 1, i + (e.key === "ArrowDown" ? 1 : -1)))]; selectTrack(n); document.querySelector(`.thdr.track[data-track="${n.id}"]`).focus(); }
});


// ===== Editing: selection, cut/copy/paste/duplicate/delete, move/transpose, quantize/align, undo =====
// One snap setting drives both editors. Piano roll unit = 1/16 note (STEP px); arrangement unit = bar (BW px).
const rollSnap = () => ({ "1/16": 1, "1/8": 2, "1/4": 4, "Bar": 16, "Off": 0 })[tr.snap()];
const arrSnap = () => ({ "1/16": 1 / 16, "1/8": 1 / 8, "1/4": 1 / 4, "Bar": 1, "Off": 0 })[tr.snap()];
const snapTo = (x, q) => q ? Math.round(x / q) * q : Math.round(x * 100) / 100;
const editStatus = document.getElementById("edit-status");
const say = m => { editStatus.textContent = m; live.textContent = m; };
let focusArea = "arr", rCursor = 0, rDrag = null, rLast = null, aCursor = 13, aDrag = null, aLast = null, lastVel = .78;
const clipboard = { notes: null, clips: null };
// Undo: one snapshot per gesture/command (clips of every track + notes per track).
const H = createHistory();  // undo/redo + state ids (ui/state/history.ts)
// Undo: one whole-song snapshot per gesture or command (tracks, clips, notes, devices, automation, mixer).
// No replacer: view fields live in ui/state/view.ts, and a replacer function runs per key (212 ms per knob step on a
// 4 MB song vs ~11 ms native, bench/ui-profile.ts).
const snap = () => JSON.stringify({ tracks: raw(tracks), master: raw(master), sel: selected.id, armed: armed?.id });
function restore(js) { batch(() => restore1(js)); }  // one notification pass for the whole restore
function restore1(js) {
  const d = JSON.parse(js);
  overrides.clear();  // as before the view store: restored tracks follow their automation lanes again
  // the roll's clip by name/start (as before: restored clips merge by position, an object reference could now hold
  // another clip's values)
  for (const [id, c] of rollClips) if (c) rollClips.set(id, { name: c.name, start: c.start });
  restoreSong(song, d);  // tracks by id, then master; unchanged objects keep identity
  selected = tracks.find(t => t.id === d.sel) || (d.sel === "master" ? master : tracks[0]); armed = tracks.find(t => t.id === d.armed) || null;
  rebuildTracks(); selectTrack(selected); updateArmed(); if (typeof dirtyUpd === "function") dirtyUpd(); livePush();
}
function commit(before, msg) { if (!H.commit(before, snap())) return; say(msg); if (typeof dirtyUpd === "function") dirtyUpd(); livePush(); }
function undo() { const js = H.undo(snap()); if (js == null) return say("nothing to undo"); restore(js); say("undo"); }
function redo() { const js = H.redo(snap()); if (js == null) return say("nothing to redo"); restore(js); say("redo"); }


// ---- piano roll ----
const selNotes = () => rollNotes.filter(n => n.sel);
const rpos = e => { const r = gridc.getBoundingClientRect(); return [(e.clientX - r.left + rollX0) / STEP, HI - Math.floor((e.clientY - r.top) / KH)]; };
const noteHit = (st, key) => [...rollNotes].reverse().find(n => n.s < rollLen && n.n === key && st >= n.s && st < n.s + n.l);
const roll = {
  all() { rollNotes.forEach(n => n.sel = n.s < rollLen); },
  copy() { const ns = selNotes(); if (!ns.length) return say("no notes selected"); const s0 = Math.min(...ns.map(n => n.s)); clipboard.notes = ns.map(n => ({ ...n, s: n.s - s0 })); say(`${ns.length} notes copied`); },
  del() { const k = selNotes().length; rollNotes.splice(0, rollNotes.length, ...rollNotes.filter(n => !n.sel)); say(`${k} notes deleted`); },
  cut() { roll.copy(); roll.del(); say(`${clipboard.notes?.length || 0} notes cut`); },
  paste(at = rCursor) { if (!clipboard.notes) return say("clipboard empty"); rollNotes.forEach(n => n.sel = false);
    for (const n of clipboard.notes) if (at + n.s < rollLen) rollNotes.push({ ...n, s: at + n.s, l: Math.min(n.l, rollLen - at - n.s), sel: true });
    say(`${clipboard.notes.length} notes pasted at ${barStr(at)}`); },
  dup() { const ns = selNotes(); if (!ns.length) return say("no notes selected"); const s0 = Math.min(...ns.map(n => n.s)), e0 = Math.max(...ns.map(n => n.s + n.l)), q = rollSnap() || 1;
    const at = Math.ceil(e0 / q) * q; const keep = clipboard.notes; roll.copy(); roll.paste(at); clipboard.notes = keep; say(`${ns.length} notes duplicated to ${barStr(at)}`); void s0; },
  transpose(d) { const ns = selNotes(); if (!ns.length) return say("no notes selected"); if (ns.some(n => n.n + d < 0 || n.n + d > 127)) return say("out of MIDI range");
    ns.forEach(n => n.n += d); say(`transposed ${d > 0 ? "+" : ""}${d}`); scrollToNotes(ns); },
  move(d) { const ns = selNotes(); if (!ns.length) return; if (ns.some(n => n.s + d < 0 || n.s + n.l + d > rollLen)) return say("clip edge"); ns.forEach(n => n.s += d); say(`moved ${d > 0 ? "+" : ""}${d} steps`); },
  resize(d) { selNotes().forEach(n => n.l = Math.max(rollSnap() || .25, Math.min(rollLen - n.s, n.l + d))); say("length changed"); },
  quant(lengths) { const q = rollSnap() || 1, str = +document.getElementById("qstr").value, ns = selNotes().length ? selNotes() : rollNotes;
    for (const n of ns) { n.s += (Math.round(n.s / q) * q - n.s) * str; if (lengths) n.l = Math.max(q, n.l + (Math.round(n.l / q) * q - n.l) * str); n.s = Math.max(0, n.s); }
    say(`quantized ${ns.length} notes to ${tr.snap()} at ${Math.round(str * 100)}%`); },
  legato() { const ns = (selNotes().length ? selNotes() : rollNotes).slice().sort((a, b) => a.s - b.s);
    ns.forEach((n, i) => { const nx = ns.slice(i + 1).find(m => m.s > n.s); n.l = (nx ? nx.s : rollLen) - n.s; }); say("legato"); },
};
const barStr = st => `${1 + Math.floor(st / 16)}.${1 + Math.floor((st % 16) / 4)}.${Math.round((st % 4) * 240)}`;
function scrollToNotes(ns) { const top = (HI - Math.max(...ns.map(n => n.n))) * KH, bot = (HI - Math.min(...ns.map(n => n.n)) + 1) * KH;
  if (top < grid.scrollTop || bot > grid.scrollTop + grid.clientHeight) grid.scrollTop = Math.max(0, top - 40); }
function rollCmd(name, arg) { const before = snap(); roll[name](arg); commit(before, editStatus.textContent); drawRoll(); renderLane(selected); }
document.querySelectorAll("#roll-tools [data-roll]").forEach(b => b.onclick = () => {
  const a = b.dataset.roll; focusArea = "roll";
  if (a.startsWith("t")) rollCmd("transpose", +a.slice(1)); else if (a === "quantlen") rollCmd("quant", true); else rollCmd(a);
  gridc.focus();
});
gridc.addEventListener("pointerdown", e => {
  focusArea = "roll"; gridc.focus(); gridc.setPointerCapture(e.pointerId);
  const [st, key] = rpos(e), n = noteHit(st, key), now = performance.now();
  const dbl = rLast && now - rLast.t < 400 && Math.hypot(e.clientX - rLast.x, e.clientY - rLast.y) < 5;  // two clicks, no drag
  rLast = null;
  if (n) {
    if (dbl) { const before = snap(); n.sel = true; roll.del(); commit(before, "note deleted"); rLast = null; drawRoll(); return; }
    if (e.shiftKey) { n.sel = !n.sel; drawRoll(); return; }
    if (!n.sel) { rollNotes.forEach(x => x.sel = false); n.sel = true; }
    lastVel = n.v;
    rDrag = { mode: st > n.s + n.l - .35 ? "resize" : "move", st0: st, key0: key, before: snap(), orig: selNotes().map(x => [x, x.s, x.l, x.n]), hit: n, l0: n.l };
  } else {
    if (dbl) { const before = snap(), q = rollSnap() || 1, s0 = Math.floor(st / q) * q;
      rollNotes.forEach(x => x.sel = false); rollNotes.push({ s: s0, l: q, n: key, v: lastVel, sel: true }); commit(before, `note ${NAMES[key % 12]}${Math.floor(key / 12) - 1} added at ${barStr(s0)}`); rLast = null; drawRoll(); return; }
    if (!e.shiftKey) rollNotes.forEach(x => x.sel = false);
    rCursor = Math.max(0, Math.min(rollLen - 1, Math.floor(st / (rollSnap() || 1)) * (rollSnap() || 1)));
    rDrag = { mode: "marquee", st0: st, key0: key, st1: st, key1: key, keep: new Set(selNotes()) };
  }
  drawRoll();
});
gridc.addEventListener("pointermove", e => {
  if (!rDrag) return; const [st, key] = rpos(e); if (!rDrag.moved && Math.hypot(e.movementX, e.movementY) < 1 && st === rDrag.st0) return; rDrag.moved = true; const q = rollSnap();
  if (rDrag.mode === "move") { let ds = snapTo(st - rDrag.st0, q), dk = key - rDrag.key0;
    ds = Math.max(-Math.min(...rDrag.orig.map(o => o[1])), Math.min(rollLen - Math.max(...rDrag.orig.map(o => o[1] + o[2])), ds));
    dk = Math.max(LO - Math.min(...rDrag.orig.map(o => o[3])), Math.min(HI - Math.max(...rDrag.orig.map(o => o[3])), dk));
    for (const [n, s0, , k0] of rDrag.orig) { n.s = s0 + ds; n.n = k0 + dk; } }
  else if (rDrag.mode === "resize") {
    // the grabbed note snaps; the other selected notes scale by its factor (to the tick), Alt: the same amount
    const dl = snapTo(st - rDrag.st0, q), lh = Math.max(q || .25, Math.min(rollLen - rDrag.hit.s, rDrag.l0 + dl)), f = lh / rDrag.l0;
    rDrag.alt = e.altKey; rDrag.f = f;
    for (const [n, s0, l0] of rDrag.orig) n.l = n === rDrag.hit ? lh : Math.max(.25, Math.min(rollLen - s0, e.altKey ? l0 + dl : Math.round(l0 * f * 240) / 240));
    say(rDrag.orig.length > 1 && !e.altKey ? `length \u00d7${f.toFixed(2)} (${f >= 1 ? "+" : "\u2212"}${Math.abs(Math.round((f - 1) * 100))}%)` : `length ${+lh.toFixed(2)} steps`); }
  else { rDrag.st1 = st; rDrag.key1 = key; const [a1, b1] = [Math.min(rDrag.st0, st), Math.max(rDrag.st0, st)], [k1, k2] = [Math.min(rDrag.key0, key), Math.max(rDrag.key0, key)];
    for (const n of rollNotes) n.sel = rDrag.keep.has(n) || (n.n >= k1 && n.n <= k2 && n.s < b1 && n.s + n.l > a1); }
  drawRoll();
});
gridc.addEventListener("pointerup", e => {
  if (rDrag && !rDrag.moved) rLast = { t: performance.now(), x: e.clientX, y: e.clientY };
  if (rDrag?.before && rDrag.moved) commit(rDrag.before, rDrag.mode === "move" ? `${rDrag.orig.length} notes moved` :
    rDrag.orig.length > 1 && !rDrag.alt ? `${rDrag.orig.length} notes: length ${rDrag.f >= 1 ? "+" : "\u2212"}${Math.abs(Math.round((rDrag.f - 1) * 100))}%` : "length changed");
  if (rDrag?.mode === "marquee") say(`${selNotes().length} notes selected`);
  rDrag = null; drawRoll(); renderLane(selected);
});
gridc.addEventListener("keydown", e => {
  const mod = e.ctrlKey || e.metaKey, k = e.key.toLowerCase(); let cmd = null, arg;
  if (mod && k === "a") cmd = "all"; else if (mod && k === "c") cmd = "copy"; else if (mod && k === "x") cmd = "cut"; else if (mod && k === "v") cmd = "paste"; else if (mod && k === "d") cmd = "dup";
  else if (mod && k === "z") { e.preventDefault(); return e.shiftKey ? redo() : undo(); }
  else if (k === "delete" || k === "backspace") cmd = "del";
  else if (k === "arrowup" || k === "arrowdown") { cmd = "transpose"; arg = (k === "arrowup" ? 1 : -1) * (e.shiftKey ? 12 : 1); }
  else if ((k === "arrowleft" || k === "arrowright") && e.altKey) { cmd = "resize"; arg = (k === "arrowright" ? 1 : -1) * (rollSnap() || 1); }
  else if (k === "arrowleft" || k === "arrowright") { cmd = "move"; arg = (k === "arrowright" ? 1 : -1) * (e.shiftKey ? 16 : rollSnap() || 1); }
  else if (k === "q") { cmd = "quant"; arg = e.shiftKey; }
  else if (k === "escape") { rollNotes.forEach(n => n.sel = false); drawRoll(); return; }
  if (!cmd) return; e.preventDefault(); rollCmd(cmd, arg);
});
// Roll up/down: mouse wheel scrolls pitch (native), keys column follows; start centred on the clip's notes.
function centreRoll() { if (rollNotes.length) { const mid = rollNotes.reduce((a, n) => a + n.n, 0) / rollNotes.length; grid.scrollTop = Math.max(0, (HI - mid) * KH - grid.clientHeight / 2); } }

const velSet = document.getElementById("vel-set");
velSet.addEventListener("change", () => { const v = Math.max(1, Math.min(127, Math.round(+velSet.value || 0))); if (!v || !selNotes().length) return;
  const before = snap(), ns = selNotes(); ns.forEach(n => n.v = v / 127); lastVel = v / 127; commit(before, `velocity ${v} on ${ns.length} notes`); velSet.blur(); drawRoll(); });
// ---- arrangement ----
const instrTracks = () => tracks.filter(t => !t.bus);
const selClips = () => tracks.flatMap(t => t.clips.filter(c => c.sel).map(c => ({ t, c })));
const clearClipSel = () => tracks.forEach(t => t.clips.forEach(c => c.sel = false));
// Placing a clip over others trims or removes what it covers (Ableton-style), never overlaps.
function place(t, nc) {
  const out = [];
  for (const c of t.clips) {
    if (c === nc) continue;
    const a = c.start, b = c.start + c.len, x = nc.start, y = nc.start + nc.len;
    if (b <= x || a >= y) out.push(c); else if (a < x && b > y) out.push({ ...c, len: x - a }, { ...c, start: y, len: b - y, sel: false });
    else if (a < x) out.push({ ...c, len: x - a }); else if (b > y) out.push({ ...c, start: y, len: b - y });
  }
  t.clips = [...out, nc].sort((p1, p2) => p1.start - p2.start);
}
let uniq = 2;
const arr = {
  all() { tracks.forEach(t => t.clips.forEach(c => c.sel = true)); },
  copy() { const cs = selClips(); if (!cs.length) return say("no clips selected"); const i0 = Math.min(...cs.map(x => instrTracks().indexOf(x.t))), s0 = Math.min(...cs.map(x => x.c.start));
    clipboard.clips = cs.map(({ t, c }) => ({ dt: instrTracks().indexOf(t) - i0, ds: c.start - s0, len: c.len, name: c.name, pat: structuredClone(raw(patOf(t, c.name))) })); say(`${cs.length} clips copied`); },
  del() { const n = selClips().length; tracks.forEach(t => t.clips = t.clips.filter(c => !c.sel)); say(`${n} clips deleted`); },
  cut() { arr.copy(); arr.del(); say(`${clipboard.clips?.length || 0} clips cut`); },
  paste(linked, at = aCursor) { if (!clipboard.clips) return say("clipboard empty"); clearClipSel(); const it = instrTracks(), i0 = Math.max(0, it.indexOf(selected));
    for (const c of clipboard.clips) { const t = it[i0 + c.dt]; if (!t) continue;
      const name = linked ? c.name : `${c.name} ${uniq++}`; if (!t.pats?.[name]) (t.pats ??= {})[name] = structuredClone(c.pat);
      place(t, { start: at + c.ds, len: c.len, name, sel: true }); }
    say(`${clipboard.clips.length} clips pasted${linked ? " (linked)" : ""} at bar ${+at.toFixed(2)}`); },
  dup(linked) { const cs = selClips(); if (!cs.length) return say("no clips selected"); const end = Math.max(...cs.map(x => x.c.start + x.c.len)), keep = clipboard.clips, t0 = selected;
    selected = instrTracks()[Math.min(...cs.map(x => instrTracks().indexOf(x.t)))]; arr.copy(); arr.paste(linked, end); clipboard.clips = keep; selected = t0;
    say(`${cs.length} clips duplicated${linked ? " (linked)" : ""} to bar ${+end.toFixed(2)}`); },
  move(d) { const cs = selClips(); if (!cs.length) return; if (cs.some(x => x.c.start + d < 1)) return say("song start");
    cs.forEach(x => x.c.start += d); cs.forEach(x => place(x.t, x.c)); say(`moved ${d > 0 ? "+" : ""}${d} bar`); },
  track(d) { const cs = selClips(), it = instrTracks(); if (!cs.length) return; if (cs.some(x => !it[it.indexOf(x.t) + d])) return say("no track there");
    for (const x of cs) { const nt = it[it.indexOf(x.t) + d]; carry(x.t, nt, x.c.name); x.t.clips = x.t.clips.filter(c => c !== x.c); place(nt, x.c); } say(`moved to ${d > 0 ? "next" : "previous"} track`); },
  align() { const q = arrSnap() || 1, cs = selClips().length ? selClips() : tracks.flatMap(t => t.clips.map(c => ({ t, c })));
    for (const x of cs) x.c.start = Math.max(1, 1 + Math.round((x.c.start - 1) / q) * q); cs.forEach(x => place(x.t, x.c)); say(`aligned ${cs.length} clips to ${q === 1 ? "bars" : tr.snap()}`); },
};
function arrCmd(name, arg) { const before = snap(); arr[name](arg); fitSong(); commit(before, editStatus.textContent); tracks.forEach(renderLane); drawCursor(); renderDevice(); }
document.querySelectorAll("#arr-corner [data-arr]").forEach(b => b.onclick = () => { focusArea = "arr"; arrCmd(b.dataset.arr); lw.focus(); });
const lanesEl = document.getElementById("lanes"), cursorEl = Object.assign(document.createElement("div"), { className: "cursor" }); lanesEl.append(cursorEl);
function drawCursor() { cursorEl.style.left = (aCursor - 1) * BW + "px"; }
drawCursor();
const lpos = e => { const r = lanesEl.getBoundingClientRect(); return [1 + (e.clientX - r.left) / BW, e.clientY - r.top]; };
const laneAtY = y => [...lanesEl.querySelectorAll(".lane.track")].find(l => { const r = l.getBoundingClientRect(); return y >= r.top && y < r.bottom; });
lanesEl.addEventListener("pointerdown", e => {
  if (e.button !== 0 || !e.target.closest(".lane.track")) return;  // automation lanes have their own editor; right button = menu
  focusArea = "arr"; lw.focus({ preventScroll: true }); lanesEl.setPointerCapture(e.pointerId);
  const el = e.target.closest(".clip"), [bar, y] = lpos(e), t = tracks.find(x => x.id === e.target.closest(".lane.track").dataset.track), now = performance.now();
  if (el) {
    const c = el._clip;
    if (!e.target.closest(".lbl, .rz")) return bodyDown(e, t, c, bar, y);  // clip body: click selects, drag selects a range
    if (e.shiftKey) { c.sel = !c.sel; renderLane(t); return; }
    if (!c.sel) { clearClipSel(); c.sel = true; }
    aDrag = { mode: e.target.classList.contains("rz") ? "resize" : "move", alt: e.altKey, bars0: BARS, bar0: bar, before: snap(), t0: t, orig: selClips().map(x => ({ ...x, start: x.c.start, len: x.c.len })) };
    rollClips.set(t.id, c); selectTrack(t); if (!document.getElementById("p-roll").hidden) centreRoll();
  } else {
    if (!e.shiftKey) clearClipSel();
    aCursor = Math.max(1, 1 + Math.floor((bar - 1) / (arrSnap() || 1)) * (arrSnap() || 1)); drawCursor();
    aDrag = { mode: "marquee", bar0: bar, y0: y, keep: new Set(selClips().map(x => x.c)), box: Object.assign(document.createElement("div"), { className: "marquee" }) };
    if (selected !== t) selectTrack(t);
  }
  tracks.forEach(renderLane);
});
// Press on a clip body (Ableton: the title bar moves, the body selects). Click = select the clip; drag = range.
function bodyDown(e, t, c, bar, y) {
  aCursor = Math.max(1, 1 + Math.floor((bar - 1) / (arrSnap() || 1)) * (arrSnap() || 1)); drawCursor();
  aDrag = { mode: "marquee", bar0: bar, y0: y, x: e.clientX, yc: e.clientY, keep: new Set(e.shiftKey ? selClips().map(x => x.c) : []), pend: { c, t, shift: e.shiftKey },
    box: Object.assign(document.createElement("div"), { className: "marquee" }) };
}
lanesEl.addEventListener("pointermove", e => {
  if (!aDrag) return; const [bar, y] = lpos(e), q = arrSnap();
  if (!aDrag.moved && aDrag.pend && Math.hypot(e.clientX - aDrag.x, e.clientY - aDrag.yc) < 4) return;
  if (!aDrag.moved && aDrag.pend && !aDrag.pend.shift) clearClipSel();
  aDrag.moved = true;
  if (aDrag.mode === "move") {
    let d = snapTo(bar - aDrag.bar0, q); d = Math.max(1 - Math.min(...aDrag.orig.map(o => o.start)), d);
    const it = instrTracks(), lane = laneAtY(e.clientY), tt = lane && tracks.find(x => x.id === lane.dataset.track);
    const dt = tt && !tt.bus ? it.indexOf(tt) - it.indexOf(aDrag.t0) : 0, ok = aDrag.orig.every(o => it[it.indexOf(o.t) + dt]);
    for (const o of aDrag.orig) { o.c.start = o.start + d; const nt = ok ? it[it.indexOf(o.t) + dt] : o.t; if (nt !== o.cur) { carry(o.t, nt, o.c.name); (o.cur || o.t).clips = (o.cur || o.t).clips.filter(c => c !== o.c); nt.clips.push(o.c); o.cur = nt; } }
    aDrag.dt = ok ? dt : 0;
  } else if (aDrag.mode === "resize") { const d = snapTo(bar - aDrag.bar0, q); for (const o of aDrag.orig) { o.c.len = Math.max(q || .25, o.len + d); if (aDrag.alt) o.c.loop = true; } }
  else { const x0 = (Math.min(aDrag.bar0, bar) - 1) * BW, x1 = (Math.max(aDrag.bar0, bar) - 1) * BW, y0 = Math.min(aDrag.y0, y), y1 = Math.max(aDrag.y0, y);
    Object.assign(aDrag.box.style, { left: x0 + "px", top: y0 + "px", width: x1 - x0 + "px", height: y1 - y0 + "px" }); if (!aDrag.box.isConnected) lanesEl.append(aDrag.box);
    const lr = lanesEl.getBoundingClientRect();
    for (const t of tracks) for (const c of t.clips) { const ly = laneEl.get(t.id)?.offsetTop ?? -1, lh = laneEl.get(t.id)?.offsetHeight ?? 0, cx0 = (c.start - 1) * BW, cx1 = cx0 + c.len * BW;
      c.sel = aDrag.keep.has(c) || (cx0 < x1 && cx1 > x0 && ly < y1 && ly + lh > y0); } void lr; }
  if (aDrag.mode !== "marquee") fitSong(true);
  tracks.forEach(renderLane);
});
lanesEl.addEventListener("pointerup", () => {
  if (!aDrag) return;
  if (aDrag.mode === "move" && aDrag.moved) { for (const o of aDrag.orig) place(o.cur || o.t, o.c); commit(aDrag.before, `${aDrag.orig.length} clips moved${aDrag.dt ? ` ${aDrag.dt > 0 ? "down" : "up"} ${Math.abs(aDrag.dt)} track` : ""}`); }
  if (aDrag.mode === "resize" && aDrag.moved) { for (const o of aDrag.orig) place(o.t, o.c); commit(aDrag.before, "clip length changed"); }
  if (aDrag.mode === "marquee") { aDrag.box.remove(); if (aDrag.moved) say(`${selClips().length} clips selected`);
    else if (aDrag.pend) { const { c, t, shift } = aDrag.pend; if (shift) c.sel = !c.sel; else { clearClipSel(); c.sel = true; }
      rollClips.set(t.id, c); selectTrack(t); if (!document.getElementById("p-roll").hidden) { centreRoll(); } } }
  if (aDrag.mode === "move" || aDrag.mode === "resize") { fitSong(); if (aDrag.moved && BARS > aDrag.bars0) say(`${editStatus.textContent} · song extended to ${BARS} bars`); }
  aDrag = null; tracks.forEach(renderLane); renderDevice();
});
lw.addEventListener("keydown", e => { if (e.target === lw) arrKeys(e); });
lanesEl.addEventListener("mousedown", e => { if (e.button === 0 && e.target.closest(".lane.track")) e.preventDefault(); });
function arrKeys(e, hdrTrack) {
  const mod = e.ctrlKey || e.metaKey, k = e.key.toLowerCase(); let cmd = null, arg;
  if (mod && k === "a") cmd = "all"; else if (mod && k === "c") cmd = "copy"; else if (mod && k === "x") cmd = "cut";
  else if (mod && k === "v") { cmd = "paste"; arg = e.shiftKey; } else if (mod && k === "d") { cmd = "dup"; arg = e.shiftKey; }
  else if (mod && k === "z") { e.preventDefault(); return e.shiftKey ? redo() : undo(); }
  else if (k === "delete" || k === "backspace") cmd = "del";
  else if (k === "arrowleft" || k === "arrowright") { cmd = "move"; arg = (k === "arrowright" ? 1 : -1) * (e.shiftKey ? 4 : arrSnap() || 1); }
  else if (k === "arrowup" || k === "arrowdown") { cmd = "track"; arg = k === "arrowdown" ? 1 : -1; }
  else if (k === "q") cmd = "align";
  else if (k === "escape") { clearClipSel(); tracks.forEach(renderLane); return; }
  if (hdrTrack && cmd === "all") { e.preventDefault(); clearClipSel(); hdrTrack.clips.forEach(c => c.sel = true); tracks.forEach(renderLane); return say(`${hdrTrack.clips.length} clips of ${hdrTrack.name} selected`); }
  if (hdrTrack && (cmd === "move" || cmd === "track")) return;  // arrows on a header move between tracks
  if (!cmd) return; e.preventDefault(); arrCmd(cmd, arg);
}

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
  for (const x of tabs) { const on = x === t; x.setAttribute("aria-selected", on); x.tabIndex = on ? 0 : -1; document.getElementById(x.getAttribute("aria-controls")).hidden = !on; }
  if (t.id === "tab-roll") { drawRoll(); centreRoll(); }
  setMixerShown(t.id === "tab-mix");  // the mixer is mounted only while its tab shows
  updateWatch();
}
tabs.forEach((t, i) => { t.addEventListener("click", () => show(t)); t.addEventListener("keydown", e => {
  if (e.key === "ArrowRight" || e.key === "ArrowLeft") { const n = tabs[(i + (e.key === "ArrowRight" ? 1 : tabs.length - 1)) % tabs.length]; n.focus(); show(n); }
}); });
const initial = new URLSearchParams(location.search).get("tab");
show(tabs.find(t => t.id === "tab-" + initial) || tabs[1]);

// Splitters (same behaviour as shader-viewer: drag + arrows)
function splitter(el, cssVar, axis, min, max) {
  const root = document.documentElement, key = "daw" + cssVar, saved = store.get(key);
  if (saved) root.style.setProperty(cssVar, saved);
  const aria = () => { const v = Math.round(parseFloat(getComputedStyle(root).getPropertyValue(cssVar)) || (axis === "x" ? 200 : 360)); el.setAttribute("aria-valuenow", v); el.setAttribute("aria-valuemin", min); el.setAttribute("aria-valuemax", max); };
  aria();
  new MutationObserver(() => { const v = root.style.getPropertyValue(cssVar); if (v) store.set(key, v); aria(); }).observe(root, { attributes: true, attributeFilter: ["style"] });
  el.addEventListener("pointerdown", e => {
    el.setPointerCapture(e.pointerId);
    const mv = ev => { const v = Math.min(max, Math.max(min, axis === "x" ? ev.clientX : ev.clientY - document.getElementById("work").getBoundingClientRect().top)); root.style.setProperty(cssVar, v + "px"); };
    el.addEventListener("pointermove", mv); el.addEventListener("pointerup", () => el.removeEventListener("pointermove", mv), { once: true });
  });
  el.addEventListener("keydown", e => {
    const cur = parseFloat(getComputedStyle(root).getPropertyValue(cssVar)) || (axis === "x" ? 200 : 360);
    const d = (e.shiftKey ? 40 : 10) * ((e.key === "ArrowRight" || e.key === "ArrowDown") ? 1 : (e.key === "ArrowLeft" || e.key === "ArrowUp") ? -1 : 0);
    if (d) { e.preventDefault(); root.style.setProperty(cssVar, Math.min(max, Math.max(min, cur + d)) + "px"); }
  });
}
splitter(document.getElementById("split-v"), "--browser-w", "x", 140, 360);
splitter(document.getElementById("split-h"), "--arr-h", "y", 120, 700);

// ===== Transport: song length, loop, seek, playback (clock + meters + a small WebAudio preview voice per track) =====
function onTempo(v) { tr.setTempo(v); showPos(); say(`tempo ${v} BPM`); dirtyUpd(); livePush(); }
// Time signature: beats per bar in the signature's unit (7/8 → 7 eighths); one quarter = 960 ticks.
const sig = () => tr.sig().split("/").map(Number), beatsPerBar = () => sig()[0], quartersPerBar = () => sig()[0] * 4 / sig()[1];
const fmtPos = b => { const n = beatsPerBar(), t = Math.max(0, b - 1) * n, beat = Math.floor(t); return `${1 + Math.floor(beat / n)}.${1 + beat % n}.${Math.floor((t - beat) * 960 * 4 / sig()[1])}`; };
function onSig(v) { tr.setSig(v); document.documentElement.style.setProperty("--beats", beatsPerBar()); showPos(); dirtyUpd(); say(`time signature ${v}: beat grid and positions follow; the piano roll keeps 16 steps per bar`); }
const fmtBars = b => +b.toFixed(2);
const afterEl = Object.assign(document.createElement("div"), { className: "after-end" }); lanes.append(afterEl);
// Grow the song to cover the last clip (never shrinks by itself).
function fitSong(quiet) {
  const end = Math.max(1, ...tracks.flatMap(t => t.clips.map(c => c.start + c.len))), need = Math.ceil(end - 1 - 1e-9);
  if (need > BARS) { BARS = need; relayout(); if (!quiet) say(`${editStatus.textContent} · song extended to ${BARS} bars`); }
}
function relayout() {
  const W = (BARS + PAD_BARS) * BW;
  ruler.style.width = lanes.style.width = W + "px";
  ruler.querySelectorAll(".bar, .end").forEach(e => e.remove());
  const every = BW < 12 ? 8 : BW < 20 ? 4 : BW < 30 ? 2 : 1;
  for (let b1 = 0; b1 < BARS + PAD_BARS; b1++) { if (b1 % every) continue; const d = document.createElement("div"); d.className = "bar num"; d.style.left = b1 * BW + "px"; d.textContent = b1 + 1; if (b1 >= BARS) d.style.opacity = .4; ruler.prepend(d); }
  const end = Object.assign(document.createElement("div"), { className: "end", textContent: "end" }); end.style.left = BARS * BW + "px"; ruler.append(end);
  afterEl.style.left = BARS * BW + "px"; afterEl.style.width = PAD_BARS * BW + "px";
  tr.setBars(BARS); laneVals.forEach(l => l.draw());
}
function onLength(v) {
  const last = Math.ceil(Math.max(1, ...tracks.flatMap(t => t.clips.map(c => c.start + c.len))) - 1 - 1e-9);
  if (v < last) { BARS = last; say(`length kept at ${last} bars: clips reach bar ${last + 1}`); } else { BARS = v; say(`song length ${v} bars`); }
  relayout();
}
// Loop: top strip of the ruler. Drag the bar to move, its edges to resize, empty strip to draw a new loop.
const loopStrip = Object.assign(document.createElement("div"), { className: "loopstrip" }); ruler.prepend(loopStrip);
lp.tabIndex = 0; lp.setAttribute("role", "group");
const loopSnap = () => arrSnap() || 0;
const snapBar = b => 1 + snapTo(b - 1, loopSnap());
function updateLoop(msg) {
  lp.style.left = shade.style.left = (LOOP[0] - 1) * BW + "px"; lp.style.width = shade.style.width = (LOOP[1] - LOOP[0]) * BW + "px";
  lp.classList.toggle("off", !loopOn); shade.classList.toggle("off", !loopOn);
  lp.setAttribute("aria-label", `Loop ${fmtBars(LOOP[0])} to ${fmtBars(LOOP[1])}${loopOn ? "" : ", off"}. Arrows move it, Shift+arrows change its end.`);
  tr.setLoop([LOOP[0], LOOP[1]]); tr.setLoopOn(loopOn);  // the bar shows them
  if (!document.getElementById("p-roll").hidden && rollClip) drawRoll();
  if (msg) say(msg);
  if (typeof liveLoop === "function") liveLoop();
}
let loopOn = false, lDrag = null;  // loop off by default: playback runs through the song (L or ⟲ turns it on)
const rbar = e => 1 + (e.clientX - ruler.getBoundingClientRect().left) / BW;
ruler.addEventListener("pointerdown", e => {
  ruler.setPointerCapture(e.pointerId); const b1 = rbar(e), top = e.clientY - ruler.getBoundingClientRect().top < 10;
  if (!top) { lDrag = { seek: true }; seek(b1); return; }
  const x0 = (LOOP[0] - 1) * BW, x1 = (LOOP[1] - 1) * BW, x = (b1 - 1) * BW;
  const mode = Math.abs(x - x0) < 6 ? "a" : Math.abs(x - x1) < 6 ? "b" : x > x0 && x < x1 ? "move" : "new";
  lDrag = { mode, b0: b1, a: LOOP[0], b: LOOP[1], anchor: snapBar(b1) };
  if (mode === "new") { LOOP[0] = LOOP[1] = lDrag.anchor; }
  loopOn = true; updateLoop();
});
ruler.addEventListener("pointermove", e => {
  const b1 = rbar(e);
  if (!lDrag) { const x = (b1 - 1) * BW, top = e.clientY - ruler.getBoundingClientRect().top < 10, x0 = (LOOP[0] - 1) * BW, x1 = (LOOP[1] - 1) * BW;
    ruler.style.cursor = !top ? "pointer" : Math.abs(x - x0) < 6 || Math.abs(x - x1) < 6 ? "ew-resize" : x > x0 && x < x1 ? "grab" : "copy"; return; }
  if (lDrag.seek) return seek(b1);
  const min = loopSnap() || .25, endMax = BARS + PAD_BARS + 1;
  if (lDrag.mode === "move") { const d = Math.max(1 - lDrag.a, Math.min(endMax - lDrag.b, snapTo(b1 - lDrag.b0, loopSnap()))); LOOP[0] = lDrag.a + d; LOOP[1] = lDrag.b + d; }
  else if (lDrag.mode === "a") LOOP[0] = Math.max(1, Math.min(LOOP[1] - min, snapBar(b1)));
  else if (lDrag.mode === "b") LOOP[1] = Math.min(endMax, Math.max(LOOP[0] + min, snapBar(b1)));
  else { const c1 = Math.max(1, snapBar(b1)); [LOOP[0], LOOP[1]] = c1 >= lDrag.anchor ? [lDrag.anchor, Math.max(c1, lDrag.anchor + min)] : [c1, lDrag.anchor]; }
  updateLoop(); ruler.style.cursor = lDrag.mode === "move" ? "grabbing" : "ew-resize";
});
ruler.addEventListener("pointerup", () => { if (lDrag && !lDrag.seek) updateLoop(`loop ${fmtBars(LOOP[0])} – ${fmtBars(LOOP[1])}`); lDrag = null; });
lp.addEventListener("keydown", e => {
  const d = (e.key === "ArrowRight" ? 1 : e.key === "ArrowLeft" ? -1 : 0) * (loopSnap() || 1); if (!d) return; e.preventDefault();
  if (e.shiftKey) LOOP[1] = Math.max(LOOP[0] + (loopSnap() || .25), LOOP[1] + d); else if (LOOP[0] + d >= 1) { LOOP[0] += d; LOOP[1] += d; }
  updateLoop(`loop ${fmtBars(LOOP[0])} – ${fmtBars(LOOP[1])}`);
});
function onLoopText(str) {
  const m = str.match(/(\d+(?:\.\d+)?)\s*[–\-—,: ]+\s*(\d+(?:\.\d+)?)/);
  if (!m || +m[1] < 1 || +m[2] <= +m[1]) { say("loop: type start – end in bars, e.g. 5 – 13"); return false; }
  LOOP[0] = +m[1]; LOOP[1] = +m[2]; loopOn = true; updateLoop(`loop ${fmtBars(LOOP[0])} – ${fmtBars(LOOP[1])}`); return true;
}
const toggleLoop = () => { loopOn = !loopOn; updateLoop(loopOn ? "loop on" : "loop off"); };
const loopSelection = () => { const cs = selClips(); if (!cs.length) return say("select clips to loop them");
  LOOP[0] = Math.min(...cs.map(x => x.c.start)); LOOP[1] = Math.max(...cs.map(x => x.c.start + x.c.len)); loopOn = true; updateLoop(`loop set to the selection ${fmtBars(LOOP[0])} – ${fmtBars(LOOP[1])}`); };
// Follow pauses when the user scrolls or zooms a view by hand (Ableton's rule); ⇥/F, play or a ruler click resume it.
let followHold = false;
function holdFollow() {
  if (followHold || !playing || !tr.follow()) return;
  followHold = true; tr.setFollowHeld(true); say("follow paused while you look around · F resumes");
}
function resumeFollow() { followHold = false; tr.setFollowHeld(false); }
function toggleFollow() {
  if (followHold) { resumeFollow(); tr.setFollow(true); say("follow resumed"); showPos(); return; }
  tr.setFollow(!tr.follow()); resumeFollow();
}
for (const el of [lw, grid, vel, keys, hdrs, ruler]) {
  el.addEventListener("wheel", holdFollow, { passive: true });
  el.addEventListener("touchmove", holdFollow, { passive: true });
}
// Dragging a scrollbar: the press lands on the scroll container itself, not on its content.
for (const el of [lw, grid]) el.addEventListener("pointerdown", e => { if (e.target === el) holdFollow(); });

// Mute / solo on headers and strips (one state per track, both views in sync).
document.addEventListener("click", e => {
  const b = e.target.closest('button[aria-label^="Mute "], button[aria-label^="Solo "]'); if (!b) return;
  const solo = b.getAttribute("aria-label").startsWith("Solo"), name = b.getAttribute("aria-label").slice(5), t = tracks.find(x => x.name === name); if (!t) return;
  const before = snap(); t[solo ? "solo" : "mute"] = !t[solo ? "solo" : "mute"]; if (!solo) setTimeout(() => commit(before, `${name} ${t.mute ? "muted" : "unmuted"}`));
  document.querySelectorAll(`button[aria-label="${solo ? "Solo" : "Mute"} ${name}"]`).forEach(x => x.setAttribute("aria-pressed", !!t[solo ? "solo" : "mute"]));
  say(`${name} ${solo ? (t.solo ? "soloed" : "unsoloed") : (t.mute ? "muted" : "unmuted")}`); livePush();
});
// Audible = not muted, and soloed, a send source of a soloed track, or a bus it sends to when any solo is on (model.ts).
const audible = t => M.audible(tracks, t);

// Clock. Position in bars; 4/4 at the tempo field. The loop wraps when the playhead reaches its end.
let playing = false, lastT = 0, raf = 0;
const barsPerSec = () => (tr.tempo() || 128) / 60 / quartersPerBar();
function moveRollPh() {
  const el = document.getElementById("roll-ph"); if (!rollClip) { el.hidden = true; return; }
  let x = songStep(SONG_POS); if (rollClip.loop && x >= 0 && x < rollClip.len * 16) x %= rollLen; const hide = x < 0 || x > rollLen; if (el.hidden !== hide) el.hidden = hide; el.style.transform = `translateX(${x * STEP}px)`;
}
// Runs every animation frame while playing: reads layout first (still valid from the last frame), then writes
// only what changed; the playheads move by transform so no frame re-lays-out the arrangement.
function showPos() {
  liveState.setPos(SONG_POS);
  const x = (SONG_POS - 1) * BW;
  if (playing && tr.follow()) {
    const sl = lw.scrollLeft, cw = lw.clientWidth;
    // paused by a hand scroll: resumes by itself when the playhead, still in view, reaches the right edge
    if (followHold && x > sl + cw - 40 && x <= sl + cw) resumeFollow();
    if (!followHold && (x < sl || x > sl + cw - 40)) lw.scrollLeft = Math.max(0, x - 40);
    if (!followHold && !document.getElementById("p-roll").hidden && rollClip) { const rx = songStep(SONG_POS) * STEP, gl = grid.scrollLeft, gw = grid.clientWidth; if (rx >= 0 && rx <= rollLen * STEP && (rx < gl || rx > gl + gw - 30)) grid.scrollLeft = Math.max(0, rx - 30); }
  }
  ph.style.transform = ph2.style.transform = `translateX(${x}px)`;
  moveRollPh();
}
// ---- Engine transport (design D6, D8e): the AudioWorklet live host plays the song; the UI clock follows the
// position it reports. Every edit sends the exported song to the host, which applies it per track (D4). ----
const frameOf = b => Math.max(0, Math.round((b - 1) / barsPerSec() * E.rate));
const barOf = f => 1 + f / E.rate * barsPerSec();
let songMeta = st0.meta, songLen0 = st0.length0;  // song/2 meta (title, author, licence notes) and [bars, ticks] as loaded
const uiState = () => ({ tracks: raw(tracks), master: raw(master), tempo: tr.tempo() || 120, sig: sig(), bars: BARS, length0: songLen0, ...(songMeta ? { meta: songMeta } : {}) });
const cleanDev = d => { const { _edited, _dev, ...x } = d; return x; };
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
  statusAt = performance.now(); showPos(); applyAutomation(); laneVals.forEach(l => { if (!l.box.l.hidden) l.val(); });
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
document.addEventListener("keydown", e => {
  // undo/redo anywhere outside text fields (the arrangement and piano roll handle theirs and prevent the default)
  if ((e.ctrlKey || e.metaKey) && !e.defaultPrevented && !(e.target instanceof HTMLInputElement && e.target.type !== "checkbox") && !(e.target instanceof HTMLTextAreaElement)) {
    const k = e.key.toLowerCase();
    if (k === "z") { e.preventDefault(); return e.shiftKey ? redo() : undo(); }
    if (k === "y") { e.preventDefault(); return redo(); }
  }
  if (e.target instanceof HTMLInputElement || e.target instanceof HTMLSelectElement || e.target.getAttribute?.("role") === "slider") return;
  if (e.code === "Space") { e.preventDefault(); setPlay(!playing); }
  else if (e.key.toLowerCase() === "l" && (e.ctrlKey || e.metaKey)) { e.preventDefault(); loopSelection(); }
  else if (e.key.toLowerCase() === "l" && !e.altKey) toggleLoop();
  else if (e.key.toLowerCase() === "f" && !e.ctrlKey && !e.metaKey) toggleFollow();
  else if (e.key === "R" && e.shiftKey && !e.ctrlKey && !e.metaKey) setRec(!recOn);
  else if (e.key === "Home") { seek(loopOn ? LOOP[0] : 1); }
});
E.onStatus = st => { statusAt = performance.now(); meters(st); dropsCheck(); liveProbe(st.probe); loadAvg += (st.load - loadAvg) * .06; setLoad(Math.round(Math.min(1, st.load ? loadAvg : (loadAvg = 0)) * 100)); if (!st.playing && playing && st.gen === E.gen) setPlay(false); };
function tick(now) {
  const st = E.status;
  // between status messages (about every 30 ms) the clock runs on from the last reported frame
  const f = st.position + (st.playing ? (now - statusAt) / 1000 * E.rate : 0);
  let b = barOf(f);
  if (loopOn && b >= LOOP[1] && barOf(st.position) < LOOP[1]) b = LOOP[0] + (b - LOOP[1]);
  if (!loopOn && b >= BARS + 1) { SONG_POS = BARS + 1; showPos(); setPlay(false); return; }
  SONG_POS = b; if (recOn && recHold) recordPoint(recHold.t, recHold.d, recHold.p, recHold.el._value()); showPos(); applyAutomation(); laneTick(now);
  raf = requestAnimationFrame(tick);
}
let lastLane = 0;
function laneTick(now) { if (now - lastLane < 100) return; lastLane = now; laneVals.forEach(l => { if (!l.box.l.hidden) l.val(); });
  for (const t of tracks) laneEl.get(t.id)?.querySelectorAll(".clip").forEach(el => el.classList.toggle("playing", SONG_POS >= el._clip.start && SONG_POS < el._clip.start + el._clip.len)); }
// Meters: per-track peaks from the engine (master last), falling 20 dB/s.
const lvl = {};
let loadShown = -1, loadAvg = 0;  // engine load: mean render time per real time, averaged over about 0.5 s
// Audio drops: the browser's count of output underruns (silence played because samples came late), counted from
// the last Play, with the song positions where new ones appeared (the counter updates about once a second).
// The first 1.5 s after Play are not counted: a just-started output often underruns once before any sound, and the
// browser updates the counter about once a second.
let dropBase = null, dropSeen = 0, dropWhere = [], dropFrom = 0;
function dropsReset() { dropBase = E.drops(); dropSeen = 0; dropWhere = []; dropFrom = performance.now() + 1500; dropsShow(0, 0); }
function dropsCheck() {
  if (!playing) return;
  const d = E.drops(); if (!d) return;
  if (!dropBase || d.events < dropBase.events || performance.now() < dropFrom) dropBase = d;  // start-up; a new context starts from zero
  const n = d.events - dropBase.events;
  if (n > dropSeen) { dropWhere.push(fmtPos(SONG_POS)); if (dropWhere.length > 8) dropWhere.shift(); dropSeen = n; }
  dropsShow(n, d.seconds - dropBase.seconds);
}
function dropsShow(n, sec) {
  if (!E.drops()) return tr.setDrops(null);
  tr.setDrops({ n, title: n ? `Audio drops since Play: ${n}, ${Math.round(sec * 1000)} ms of silence, at ${dropWhere.join(", ")}${dropWhere.length >= 8 ? " (last 8)" : ""}. Raise Buffer, or close other heavy programs.`
    : "Audio drops since Play: none (the sound card always got its samples in time)" });
}
function setLoad(v) { if (v === loadShown) return; loadShown = v; liveState.setLoad(v); }
let meterAt = performance.now();
function meters(st) {
  const now = performance.now(), dt = Math.min(.2, (now - meterAt) / 1000); meterAt = now;
  [...tracks, master].forEach((t, i) => {
    const id = t.id, pk = [st.peaks[2 * i] || 0, st.peaks[2 * i + 1] || 0], cur = lvl[id] || [0, 0];
    const v = pk.map((p, c) => { const x = Math.max(Math.min(1, p), cur[c] * Math.pow(10, -dt)); return x < 1e-3 ? 0 : x; }); lvl[id] = v;
    liveState.setPeak(id, v[0], v[1]);  // publishes only a visible change (liveState.PEAK_STEP)
  });
}
relayout(); updateLoop(); showPos();
queueMicrotask(() => updateArmed());

// ===== Audition (never recorded): computer keyboard on the armed track, piano-roll keys, library entries, Web MIDI =====
let kbOn = true;
const heldV = new Map(), mwEl = document.getElementById("mw"), pbEl = document.getElementById("pb");
// Mod wheel and pitch bend (sliders, Web MIDI CC1 / pitch bend) set the armed track's instrument controller block
// (unisynth ctl.wheel / ctl.bend) live; never written to the song. A seek or stop restores the stored values.
function ctlLive(name, v) {
  const t = armed, d = t && instOf(t), p = d && paramOf(d, name), di = d ? engineDev(t, d) : -1;
  if (p && di >= 0 && E.node) E.setParam(trackIndex(t), di, p.index, v);
}
mwEl.addEventListener("input", () => ctlLive("ctl.wheel", +mwEl.value / 127));
pbEl.addEventListener("input", () => ctlLive("ctl.bend", (+pbEl.value + 8192) / 16383));
mwEl.title = "Mod wheel: the armed track's ctl.wheel (audition, not recorded)";
pbEl.title = "Pitch bend: the armed track's ctl.bend, ± its bend range (audition, not recorded; springs back)";
function noteOn(t, note, vel, key) {
  if (!t || t.bus || t.master || note < 0 || note > 127 || !instOf(t)) return; noteOff(key);
  const ti = trackIndex(t);
  heldV.set(key, { ti, note }); keyLight(note, true);
  ensureAudio().then(ok => { if (ok && heldV.get(key)?.note === note) E.noteOn(ti, note, vel); });
}
function noteOff(key) { const h = heldV.get(key); if (!h) return; if (E.node) E.noteOff(h.ti, h.note); heldV.delete(key); if (![...heldV.values()].some(x => x.note === h.note)) keyLight(h.note, false); }
function keyLight(note, on) { keys.children[HI - note]?.classList.toggle("on", on); }
// Library: "Audition" plays C3 on the selected track's instrument (load the entry to hear its own sound)
function auditionEntry(src, it, done) {
  if (src.id === "gsm") { gsmOf(it.gsm).then(b64 => b64 && playSample(b64)); return done(); }
  const n = src.id === "gm" && it.gm?.drums ? 36 : 48;
  noteOn(selected, n, kbVel, "lib"); setTimeout(() => { noteOff("lib"); done(); }, 900);
}
// Computer keyboard as a tracker's note keyboard (FastTracker 2 / Renoise layout, by physical key so QWERTZ and
// other layouts play the same): Z S X D C V G B H N J M = one octave from C, Q 2 W 3 E R 5 T 6 Y 7 U I 9 O 0 P = the
// next octave and a half. It plays the selected track (arming follows selection). Octave [ ] (PageDown/PageUp,
// numpad / *), velocity ; ' (numpad − +), ` or the ⌨ button toggles; Shift/Ctrl/Alt combinations stay shortcuts.
const KB_LO = ["KeyZ", "KeyS", "KeyX", "KeyD", "KeyC", "KeyV", "KeyG", "KeyB", "KeyH", "KeyN", "KeyJ", "KeyM"];
const KB_HI = ["KeyQ", "Digit2", "KeyW", "Digit3", "KeyE", "KeyR", "Digit5", "KeyT", "Digit6", "KeyY", "Digit7", "KeyU", "KeyI", "Digit9", "KeyO", "Digit0", "KeyP"];
const kbNote = code => { const i = KB_LO.indexOf(code); if (i >= 0) return i; const j = KB_HI.indexOf(code); return j >= 0 ? 12 + j : -1; };
const OCT_DN = ["BracketLeft", "PageDown", "NumpadDivide"], OCT_UP = ["BracketRight", "PageUp", "NumpadMultiply"];
const VEL_DN = ["Semicolon", "NumpadSubtract"], VEL_UP = ["Quote", "NumpadAdd"];
const typing = e => e.target instanceof HTMLInputElement || e.target instanceof HTMLSelectElement || e.target instanceof HTMLTextAreaElement || e.target.isContentEditable;
const kbActive = e => kbOn && armed && !e.ctrlKey && !e.metaKey && !e.altKey && !e.shiftKey && !typing(e) && !e.target.closest?.(".menu");
function toggleKb() { kbOn = !kbOn; document.getElementById("kb-btn").setAttribute("aria-pressed", kbOn); say(kbOn ? "computer keyboard plays the selected track (Z–M, Q–P)" : "computer keyboard off: letters are shortcuts again"); }
window.addEventListener("keydown", e => {
  if (e.code === "Backquote" && !e.ctrlKey && !e.metaKey && !typing(e)) { e.preventDefault(); toggleKb(); return; }
  if (!kbActive(e)) return;
  const i = kbNote(e.code), stop = () => { e.preventDefault(); e.stopImmediatePropagation(); };
  if (i >= 0) { stop(); if (!e.repeat) noteOn(armed, 12 * kbOct + i, kbVel, "k" + e.code); return; }
  if (OCT_DN.includes(e.code) || OCT_UP.includes(e.code)) { stop(); kbOct = Math.max(0, Math.min(8, kbOct + (OCT_UP.includes(e.code) ? 1 : -1))); updateArmed(); return; }
  if (VEL_DN.includes(e.code) || VEL_UP.includes(e.code)) { stop(); kbVel = Math.max(1, Math.min(127, kbVel + (VEL_UP.includes(e.code) ? 16 : -16))); updateArmed(); return; }
}, true);
window.addEventListener("keyup", e => { if (kbNote(e.code) >= 0) noteOff("k" + e.code); }, true);
window.addEventListener("blur", () => [...heldV.keys()].forEach(noteOff));
// Piano-roll keys: press = audition on the selected track
keys.addEventListener("pointerdown", e => { const b = e.target.closest("button"); if (!b) return; e.preventDefault(); keys.setPointerCapture(e.pointerId); noteOn(selected, HI - [...keys.children].indexOf(b), kbVel, "mouse"); });
for (const ev of ["pointerup", "pointercancel"]) keys.addEventListener(ev, () => noteOff("mouse"));
keys.addEventListener("keydown", e => { const b = e.target.closest("button"); if (b && (e.key === "Enter" || e.key === " ") && !e.repeat) { e.preventDefault(); noteOn(selected, HI - [...keys.children].indexOf(b), kbVel, "kbkey"); } });
keys.addEventListener("keyup", () => noteOff("kbkey"));
// Web MIDI (asks the browser on first use): notes to the armed track, CC1 to MW, pitch bend to PB
function connectMidi() {
  const info = document.getElementById("midi-info");
  if (!navigator.requestMIDIAccess) { info.textContent = "MIDI: not available in this browser"; return; }
  navigator.requestMIDIAccess().then(m => {
    const hook = () => { let n = 0; for (const i of m.inputs.values()) { i.onmidimessage = midiMsg; n++; } info.textContent = n ? `MIDI: ${n} input${n > 1 ? "s" : ""} → ${armed ? armed.name : "arm a track"}` : "MIDI: no input connected"; };
    hook(); m.onstatechange = hook;
  }).catch(() => { info.textContent = "MIDI: permission denied"; });
}
function midiMsg(e) {
  const [st, d1, d2] = e.data, cmd = st & 0xf0;
  if (cmd === 0x90 && d2) noteOn(armed, d1, d2, "m" + d1);
  else if (cmd === 0x80 || (cmd === 0x90 && !d2)) noteOff("m" + d1);
  else if (cmd === 0xb0 && d1 === 1) { mwEl.value = d2; mwEl.dispatchEvent(new Event("input")); }
  else if (cmd === 0xe0) { pbEl.value = ((d2 << 7) | d1) - 8192; pbEl.dispatchEvent(new Event("input")); }
}
document.getElementById("kb-btn").onclick = toggleKb;
document.getElementById("midi-info").onclick = connectMidi;
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
  const st = M.fromSong(s), d = JSON.parse(snap()); songMeta = st.meta; songLen0 = st.length0;
  d.tracks = st.tracks; d.sel = (st.tracks.find(t => !t.bus) || st.tracks[0])?.id ?? "master"; d.armed = st.tracks.find(t => t.devices.some(x => x.kind === 64))?.id;
  Object.keys(master).forEach(k => delete master[k]); Object.assign(master, st.master);
  tr.setTempo(st.tempo); const v = st.sig.join("/"); tr.ensureSig(v); document.documentElement.style.setProperty("--beats", beatsPerBar());
  BARS = st.bars; LOOP[0] = 1; LOOP[1] = Math.min(BARS, 4) + 1;
  restore(JSON.stringify(d));
  H.clear(); songFile = file; files.setSongFile(file); files.addSong(file); savedDepth = 0; savedAt = saved ? new Date() : null;
  relayout(); updateLoop(); SONG_POS = 1; showPos(); fitSong(true); if (saved) markSaved(); else savedId = -1; liveLoaded = false; livePush(); dirtyUpd();
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
window.addEventListener("keydown", e => { if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "s") { e.preventDefault(); e.stopImmediatePropagation(); save(); } }, true);
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
DAW.debug = { song: () => songOf(false), selected: () => selected.id, bars: () => BARS, undoDepth: () => H.depth,
  live: () => ({ probeMs: liveCost.probe / Math.max(1, liveCost.nProbe), drawMs: liveCost.draw / Math.max(1, liveCost.nDraw), maxVoices: liveCost.voices }) };
// Menus: popMenu (ui/common/Menu.tsx), right-click or Shift+F10 / ContextMenu key on headers, lanes, clips.
// ===== Tracks: add, rename, reorder, duplicate, delete =====
const TCOL = ["--t-kick", "--t-bass", "--t-lead", "--t-pad", "--t-keys", "--t-hats"];
const newId = p1 => p1 + Math.random().toString(36).slice(2, 7);
const firstBus = () => { const i = tracks.findIndex(x => x.bus); return i < 0 ? tracks.length : i; };
function trackCmd(msg, fn) { const before = snap(); const r = fn(); rebuildTracks(); selectTrack(selected); updateArmed(); commit(before, msg); return r; }
function addTrack(bus) {
  const n = tracks.filter(x => !!x.bus === !!bus).length + 1;
  const t = bus ? { id: newId("bus"), name: `Bus ${n}`, col: "--t-bus", vol: -6, bus: true, clips: [], pats: {}, devices: [{ kind: 71, values: { mode: 0, dry_wet: 1 } }], sends: [], lanes: [] }
    : { id: newId("t"), name: `Track ${n}`, col: TCOL[n % TCOL.length], vol: -6, clips: [], pats: {}, lanes: [], sends: [], devices: [initInstrument()] };
  const at = bus ? tracks.length : Math.min(firstBus(), tracks.indexOf(selected) + 1 || firstBus());
  trackCmd(`${t.name} added`, () => { tracks.splice(at, 0, t); selected = t; if (!bus) armed = t; });
  document.querySelector(`.thdr.track[data-track="${t.id}"]`)?.focus();
}
function deleteTrack(t) {
  if (t.master) return say("the master cannot be deleted");
  if (!t.bus && tracks.filter(x => !x.bus).length === 1) return say("a song keeps at least one instrument track");
  const i = tracks.indexOf(t);
  trackCmd(`${t.name} deleted (Ctrl+Z restores it)`, () => {
    tracks.splice(i, 1); for (const x of tracks) x.sends = x.sends.filter(s1 => s1[0] !== t.name);
    if (selected === t) selected = tracks[Math.min(i, tracks.length - 1)]; if (armed === t) armed = null;
  });
}
function moveTrack(t, d) {
  const i = tracks.indexOf(t), j = i + d; if (j < 0 || j >= tracks.length) return;
  trackCmd(`${t.name} moved ${d < 0 ? "up" : "down"}`, () => { tracks.splice(i, 1); tracks.splice(j, 0, t); });
  document.querySelector(`.thdr.track[data-track="${t.id}"]`)?.focus();
}
function duplicateTrack(t) {
  const c = JSON.parse(JSON.stringify(raw(t))); c.id = newId("t"); c.name = t.name + " 2"; c.clips.forEach(x => x.sel = false);
  trackCmd(`${t.name} duplicated`, () => { tracks.splice(tracks.indexOf(t) + 1, 0, c); selected = c; });
}
function renameTrack(t) { arrange.setRenamingId(t.id); }
function renameTo(t, v) {
  const old = t.name;
  trackCmd(`${old} renamed to ${v || "(empty)"}`, () => { t.name = v; for (const x of [...tracks, master]) x.sends.forEach(s1 => { if (s1[0] === old) s1[0] = v; }); });
}
function trackMenu(t, x, y) {
  const i = tracks.indexOf(t);
  popMenu(x, y, [["Rename", "F2 / double-click", () => renameTrack(t)], ["Duplicate track", "", () => duplicateTrack(t)],
    ["Move up", "Alt+↑", () => moveTrack(t, -1), i <= 0], ["Move down", "Alt+↓", () => moveTrack(t, 1), i >= tracks.length - 1], "-",
    ["Add instrument track", "", () => addTrack(false)], ["Add bus track", "", () => addTrack(true)], "-",
    ["Delete track", "Ctrl+Delete", () => deleteTrack(t)]], `${t.name} track menu`);
}
hdrs.addEventListener("contextmenu", e => { const h = e.target.closest(".thdr.track"); if (!h) return; e.preventDefault(); const t = tracks.find(x => x.id === h.dataset.track); selectTrack(t); trackMenu(t, e.clientX, e.clientY); });
hdrs.addEventListener("dblclick", e => { const nm = e.target.closest(".thdr.track .name"); if (nm) renameTrack(tracks.find(x => x.id === nm.closest(".thdr").dataset.track)); });
hdrs.addEventListener("keydown", e => {
  const h = e.target.closest(".thdr.track"); if (!h || e.target !== h) return; const t = tracks.find(x => x.id === h.dataset.track);
  if (e.key === "F2") { e.preventDefault(); renameTrack(t); }
  else if (e.altKey && (e.key === "ArrowUp" || e.key === "ArrowDown")) { e.preventDefault(); e.stopImmediatePropagation(); moveTrack(t, e.key === "ArrowUp" ? -1 : 1); }
  else if ((e.ctrlKey || e.metaKey) && (e.key === "Delete" || e.key === "Backspace")) { e.preventDefault(); e.stopImmediatePropagation(); deleteTrack(t); }
  else if (e.key === "ContextMenu" || (e.shiftKey && e.key === "F10")) { e.preventDefault(); const r = h.getBoundingClientRect(); trackMenu(t, r.left + 40, r.top + 20); }
}, true);
document.querySelector('#arr-corner button[aria-label="Add track"]').onclick = () => addTrack(false);
// ===== Clips: create, make unique, loop =====
let clipN = 1;
function createClip(t, bar) {
  if (t.bus) return say("bus tracks have no clips; they play what other tracks send");
  const q = arrSnap() >= 1 ? arrSnap() : 1, start = 1 + Math.floor((bar - 1) / q) * q, name = `clip ${clipN++}`;
  const before = snap(); clearClipSel(); (t.pats ??= {})[name] = { len: 1, notes: [], cc: { wheel: [], bend: [] } };
  const c = { start, len: 1, name, sel: true }; place(t, c); fitSong(true); rollClips.set(t.id, c);
  tracks.forEach(renderLane); selectTrack(t); commit(before, `${name} created on ${t.name} at bar ${start}; double-click the roll to add notes`);
  show(tabs.find(x => x.id === "tab-roll")); centreRoll();
}
function makeUnique(t, c) {
  if (links(t, c.name) < 2) return say(`${c.name} is not linked`);
  const before = snap(), name = `${c.name} ${uniq++}`; t.pats[name] = structuredClone(raw(patOf(t, c.name))); c.name = name;
  tracks.forEach(renderLane); genRoll(selected); commit(before, `clip is now unique: ${name}`); renderDevice();
}
function toggleClipLoop(t, c) { const before = snap(); c.loop = !c.loop; tracks.forEach(renderLane); genRoll(selected); if (!document.getElementById("p-roll").hidden) drawRoll(); commit(before, `${c.name}: ${c.loop ? `loops its ${patOf(t, c.name).len}-bar pattern` : "plays once (crop)"}`); }
// Double-click: empty lane space creates a clip; a clip opens in the piano roll. (The lanes hold pointer capture
// during a press, so the lane is found from the pointer position, not from the event target.)
// Own double-click detection: a click redraws the clips, so the browser never sees two clicks on one element.
let aLastClick = null;
lanesEl.addEventListener("pointerup", e => {
  if (e.button !== 0 || !aDrag || aDrag.moved) return;
  const now = performance.now(), dbl = aLastClick && now - aLastClick.t < 400 && Math.hypot(e.clientX - aLastClick.x, e.clientY - aLastClick.y) < 5;
  aLastClick = dbl ? null : { t: now, x: e.clientX, y: e.clientY };
  if (dbl) setTimeout(() => laneDouble(e));
}, true);
function laneDouble(e) {
  const lane = laneAtY(e.clientY); if (!lane) return; const t = tracks.find(x => x.id === lane.dataset.track), bar = lpos(e)[0];
  const c = t.clips.find(c1 => bar >= c1.start && bar < c1.start + c1.len);
  if (c) { rollClips.set(t.id, c); selectTrack(t); show(tabs.find(x => x.id === "tab-roll")); centreRoll(); return; }
  createClip(t, bar);
}
lanesEl.addEventListener("contextmenu", e => {
  const lane = e.target.closest(".lane.track"); if (!lane) return; e.preventDefault();
  const t = tracks.find(x => x.id === lane.dataset.track), el = e.target.closest(".clip"), bar = lpos(e)[0];
  if (!el) return popMenu(e.clientX, e.clientY, [["Create clip here", "double-click", () => createClip(t, bar), t.bus], ["Paste here", "Ctrl+V", () => { aCursor = 1 + Math.floor(bar - 1); drawCursor(); if (selected !== t) selectTrack(t); arrCmd("paste"); }, !clipboard.clips]], "Lane menu");
  const c = el._clip; if (!c.sel) { clearClipSel(); c.sel = true; tracks.forEach(renderLane); }
  popMenu(e.clientX, e.clientY, [["Open in piano roll", "", () => { rollClips.set(t.id, c); selectTrack(t); show(tabs.find(x => x.id === "tab-roll")); centreRoll(); }],
    ["Duplicate", "Ctrl+D", () => arrCmd("dup", false)], ["Duplicate linked", "Ctrl+Shift+D", () => arrCmd("dup", true)],
    ["Make unique", "", () => makeUnique(t, c), links(t, c.name) < 2], [c.loop ? "Loop off (crop)" : "Loop clip", "Alt+drag edge", () => toggleClipLoop(t, c)], "-",
    ["Cut", "Ctrl+X", () => arrCmd("cut")], ["Copy", "Ctrl+C", () => arrCmd("copy")], ["Delete", "Del", () => arrCmd("del")]], `Clip ${c.name} menu`);
});
// ===== Arm, volume faders (header and mixer stay in sync), solo rule, splitter memory =====
let kbOct = 4, kbVel = 100;  // computer-keyboard audition: octave of the A key (C4 = MIDI 60) and velocity
function updateArmed() {
  document.getElementById("armed-info").innerHTML = `Armed: <b>${armed ? armed.name : "none"}</b> · octave C${kbOct - 1} · vel ${kbVel}`;
  arrange.setArmedId(armed?.id ?? null);
}
document.addEventListener("click", e => {
  const b = e.target.closest(".thdr .arm"); if (!b) return; const t = tracks.find(x => x.id === b.closest(".thdr").dataset.track);
  armed = armed === t ? null : t; updateArmed(); say(armed ? `${t.name} armed: the computer keyboard (Z–M, Q–P) and MIDI play it` : "no track armed");
});
document.addEventListener("input", e => {
  const el = e.target; if (!(el instanceof HTMLInputElement) || el.type !== "range" || !/ volume$/.test(el.getAttribute("aria-label") || "")) return;
  const name = el.getAttribute("aria-label").slice(0, -7), t = [...tracks, master].find(x => x.name === name); if (!t) return;
  el._before ??= snap(); t.vol = +el.value;
  document.querySelectorAll(`input[aria-label="${CSS.escape(name)} volume"]`).forEach(x => { if (x !== el) x.value = t.vol; x.setAttribute("aria-valuetext", `${t.vol.toFixed(1)} dB`); });
});
document.addEventListener("change", e => { const el = e.target; if (el._before) { commit(el._before, `${el.getAttribute("aria-label")} ${(+el.value).toFixed(1)} dB`); el._before = null; } });
document.addEventListener("input", e => { const el = e.target; if (el instanceof HTMLInputElement && el.type === "range" && /( volume| level)$/.test(el.getAttribute("aria-label") || "")) livePush(); });
// ===== Zoom: arrangement (px per bar) and piano roll (px per 1/16, px per key), anchored at the pointer =====
const clamp = (v, a, b) => Math.min(b, Math.max(a, v));
const wheelF = e => Math.exp(-clamp(e.deltaY * (e.deltaMode === 1 ? 33 : 1), -60, 60) * .008);  // pinch ≈ small deltas, mouse notch ≈ ±100
function zoomArr(f, clientX, quiet) {
  const r = lw.getBoundingClientRect(), ax = (clientX ?? r.left + lw.clientWidth / 2) - r.left, bar = 1 + (lw.scrollLeft + ax) / BW;
  const nb = clamp(BW * f, 6, 400); if (Math.abs(nb - BW) < .01) return;
  BW = nb; document.documentElement.style.setProperty("--bar-w", BW + "px");
  relayout(); tracks.forEach(renderLane); updateLoop(); drawCursor(); showPos();
  lw.scrollLeft = (bar - 1) * BW - ax; if (!quiet) say(`arrangement zoom ${Math.round(BW / 44 * 100)}%`);
}
function zoomRoll(fx, fy, clientX, clientY, quiet) {
  const r = grid.getBoundingClientRect(), ax = (clientX ?? r.left + grid.clientWidth / 2) - r.left, ay = (clientY ?? r.top + grid.clientHeight / 2) - r.top;
  const st = (grid.scrollLeft + ax) / STEP, ky = (grid.scrollTop + ay) / KH;
  STEP = clamp(STEP * fx, 3, 120); KH = clamp(Math.round(KH * fy * 4) / 4, 6, 32);
  document.documentElement.style.setProperty("--kh", KH + "px"); keys.style.fontSize = KH < 10 ? "0" : "";
  drawRoll(); grid.scrollLeft = st * STEP - ax; grid.scrollTop = ky * KH - ay;
  if (!quiet) say(`piano roll zoom ${Math.round(STEP / 22 * 100)}% × ${Math.round(KH / 12 * 100)}%`);
}
function fitArr() {
  const cs = selClips(), a = cs.length ? Math.min(...cs.map(x => x.c.start)) : 1, b1 = cs.length ? Math.max(...cs.map(x => x.c.start + x.c.len)) : BARS + 1;
  zoomArr((lw.clientWidth - 8) / (b1 - a) / BW, undefined, true); lw.scrollLeft = (a - 1) * BW; say(cs.length ? `zoomed to ${cs.length} clips` : "zoomed to the song");
}
function fitRoll() {
  if (!rollClip) return; const ns = rollNotes.filter(n => n.s < rollLen);
  STEP = clamp((grid.clientWidth - 4) / rollLen, 3, 120);
  if (ns.length) { const lo = Math.min(...ns.map(n => n.n)) - 2, hi = Math.max(...ns.map(n => n.n)) + 2; KH = clamp(Math.floor(grid.clientHeight / (hi - lo + 1)), 6, 32); }
  document.documentElement.style.setProperty("--kh", KH + "px"); keys.style.fontSize = KH < 10 ? "0" : "";
  drawRoll(); grid.scrollLeft = 0; centreRoll(); say("zoomed to the clip");
}
// Gestures: Ctrl/⌘+wheel and trackpad pinch (Chrome/Firefox send ctrl+wheel, Safari sends gesture events).
lw.addEventListener("wheel", e => { if (e.ctrlKey || e.metaKey) { e.preventDefault(); zoomArr(wheelF(e), e.clientX, true); } }, { passive: false });
ruler.addEventListener("wheel", e => { e.preventDefault(); if (Math.abs(e.deltaX) > Math.abs(e.deltaY) && !e.ctrlKey) lw.scrollLeft += e.deltaX; else zoomArr(wheelF(e), e.clientX, true); }, { passive: false });
const rollWheel = e => {
  if (e.ctrlKey || e.metaKey) { e.preventDefault(); zoomRoll(wheelF(e), 1, e.clientX, e.clientY, true); }
  else if (e.altKey) { e.preventDefault(); zoomRoll(1, wheelF(e), e.clientX, e.clientY, true); }
};
grid.addEventListener("wheel", rollWheel, { passive: false }); vel.addEventListener("wheel", rollWheel, { passive: false });
keys.addEventListener("wheel", e => { if (e.ctrlKey || e.metaKey || e.altKey) { e.preventDefault(); zoomRoll(1, wheelF(e), undefined, e.clientY, true); } else grid.scrollTop += e.deltaY; }, { passive: false });
let g0 = 1;
for (const [el, fn] of [[lw, (sc, e) => zoomArr(g0 * sc / BW, e.clientX, true)], [grid, (sc, e) => zoomRoll(g0 * sc / STEP, 1, e.clientX, e.clientY, true)]]) {
  el.addEventListener("gesturestart", e => { e.preventDefault(); g0 = el === lw ? BW : STEP; });
  el.addEventListener("gesturechange", e => { e.preventDefault(); fn(e.scale, e); });
}

document.querySelectorAll("#roll-tools [data-zoom]").forEach(b => b.onclick = () => ({ "t+": () => zoomRoll(1.5, 1), "t-": () => zoomRoll(1 / 1.5, 1), "p+": () => zoomRoll(1, 1.25), "p-": () => zoomRoll(1, .8), fit: fitRoll })[b.dataset.zoom]());
// Keys: + / − zoom (time) and Shift+Z fits, in the area that has focus.
document.addEventListener("keydown", e => {
  if (e.target instanceof HTMLInputElement || e.target instanceof HTMLSelectElement || e.ctrlKey || e.metaKey || e.altKey) return;
  const inRoll = e.target === gridc || e.target === velc || (focusArea === "roll" && !document.getElementById("p-roll").hidden && !e.target.closest?.("#arr, #hdrs"));
  const f = e.key === "+" || e.key === "=" ? 1.5 : e.key === "-" || e.key === "_" ? 1 / 1.5 : 0;
  if (f) { e.preventDefault(); inRoll ? zoomRoll(f, 1) : zoomArr(f); }
  else if (e.key === "Z" && e.shiftKey) { e.preventDefault(); inRoll ? fitRoll() : fitArr(); }
});
}
