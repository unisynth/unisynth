// DAW UI start (change daw-solid-ts 9.1, step 9; replaces the old untyped start closure): mounts the Solid components into index.html's
// regions and wires them to the session modules in ui/app/ (engine link, transport, arrangement, clips, automation,
// piano roll, devices, mixer, files, tracks, audition). Order matters where noted: the transport bar first (the rest
// finds its ids), the roll panel last (its mount setups are bound by then). Started by main.ts.
import type { Track, TrackActions } from "./arrange/Tracks.tsx";
import type { ClipTrack } from "./arrange/Clip.tsx";
import { mountTracks } from "./arrange/Tracks.tsx";
import { mountTransport } from "./transport/TransportBar.tsx";
import { mountFiles } from "./files/Files.tsx";
import { mountLibrary } from "./library/Library.tsx";
import { mountRoll } from "./roll/Roll.tsx";
import { mountSplitter } from "./common/Splitter.tsx";
import { mountAudition } from "./footer/Audition.tsx";
import { mountMixer } from "./mixer.tsx";
import { installKeys } from "./keys.ts";
import * as tr from "./state/transport.ts";
import * as lib from "./state/library.ts";
import * as arrange from "./state/arrange.ts";
import * as au from "./state/audition.ts";
import { raw } from "./state/song.ts";
import { rollClips, selectedId, setSelectedId } from "./state/view.ts";
import { liveStats } from "./device/live.ts";
import { initParams } from "./model/params.ts";
import { instOf } from "./model/track.ts";
import { armed, type Daw, E, api, initSession, selected, song, tracks, type WithTrack } from "./app/session.ts";
import { H, initCore, redo, say, snap, undo } from "./app/core.ts";
import { setVerify } from "./state/snapshot.ts";
import { persist } from "./state/persist.ts";
import { BARS, beatsPerBar, LOOP, loopOn, playing, setBars } from "./app/transport.ts";
import { addAutoLane, addLane, applyAutomation, initAutomation, laneVals, recOn, recordFrame, setRec } from "./app/automation.ts";
import { BW, fitArr, fitSong, hdrs, initArrange, lanes, laneTick, loopSelection, lw, onLength, onLoopText, rebuildTracks, relayout, showPos, toggleFollow, toggleLoop, updateLoop, zoomArr } from "./app/arrange.ts";
import { initClips } from "./app/clips.ts";
import { initDevice, loadInto, renderAll, renderDevice } from "./app/device.ts";
import { aboutText, devLabel, headerClick, headerKey, headerMenu, initTracks, renameTo, renameTrack, selectTrack, setVolume, toggleArm, toggleMuteSolo, updateArmed } from "./app/tracks.ts";
import { bounceSong, dirtyUpd, exportSong, importFile, initFiles, markClean, newSong, openSong, save } from "./app/files.ts";
import { attachRoll, centreRoll, detachRoll, drawRoll, fitRoll, followRoll, genRoll, initRoll, inRoll, moveRollPh, rollLoopChanged, zoomRoll } from "./app/roll.ts";
import { initEngine, livePush, onBuffer, onOpenFilters, seek, setPlay, songOf, stopPressed } from "./app/engine.ts";
import { allNotesOff, auditionEntry, connectMidi, noteOff, noteOn, setBend, setWheel, toggleKb } from "./app/audition.ts";
import { mixerActions } from "./app/mixer.ts";
import { initTabs, showTab } from "./app/tabs.ts";

const el = (id: string): HTMLElement => document.getElementById(id)!;
function onTempo(v: number): void { tr.setTempo(v); showPos(); say(`tempo ${v} BPM`); dirtyUpd(); livePush(); }
function onSig(v: string): void { tr.setSig(v); document.documentElement.style.setProperty("--beats", String(beatsPerBar())); showPos(); dirtyUpd(); say(`time signature ${v}: beat grid and positions follow; the piano roll keeps 16 steps per bar`); }

export function start(daw: Daw): void {
  // Undo snapshot cache check (ui/state/snapshot.ts): on in automated browsers (tests), off for users; the
  // `daw.verifySnap` key ("1" / "0") overrides (the bench and budget tests measure with it off).
  const vs = persist.get("daw.verifySnap");  // persist: storage may throw in a sandboxed frame
  setVerify(vs ? vs === "1" : navigator.webdriver === true);
  initSession(daw);  // engine, api, song store proxies, selection (ui/app/session.ts)
  const st0 = daw.initial;
  setBars(st0.bars); LOOP[1] = Math.min(BARS, 4) + 1;
  // every device's parameters come from the engine module (design D8)
  initParams(daw.tables, (k, m, i, p) => E.physical(k, m, i, p));
  tr.setTempo(st0.tempo); tr.ensureSig(st0.sig.join("/"));
  // Transport bar first: the rest finds its ids.
  mountTransport(document.querySelector("header")!, {
    play: () => setPlay(!playing), stop: () => stopPressed(), toggleRec: () => setRec(!recOn), toggleLoop: () => toggleLoop(),
    toggleFollow: () => toggleFollow(), zoomOut: () => zoomArr(1 / 1.5), zoomIn: () => zoomArr(1.5), zoomFit: () => fitArr(),
    tempo: v => onTempo(v), sig: v => onSig(v), snap: v => tr.setSnap(v), loopText: str => onLoopText(str),
    length: v => onLength(v), buffer: v => onBuffer(v), openFilters: on => onOpenFilters(on), undo: () => undo(), redo: () => redo(),
    save: () => save(), bounce: () => bounceSong(), exportSong: () => exportSong(),
  });
  // Songs section and instrument library
  mountFiles(document.querySelector<HTMLElement>("#browser > section")!, {
    open: f => openSong(f), importFile: f => importFile(f), newSong: () => newSong(), trackCount: () => tracks.length,
  });
  mountLibrary(el("lib"), {
    hasInst: () => { selectedId(); return !!instOf(selected); }, target: () => { selectedId(); return selected.name; },
    audition: (src, it, done) => auditionEntry(src, it, done), load: (src, it) => { loadInto(selected, src, it); },
  });
  initRoll({ play: (n, key) => noteOn(selected, n, au.kbVel(), key), stop: key => noteOff(key) });
  initArrange({ rollLoop: rollLoopChanged, rollFollow: followRoll, rollPos: moveRollPh });
  initAutomation({ rebuildTracks: () => rebuildTracks() });
  // Track rows (windowed, design D6): the selected and armed rows are marked and pinned from the start.
  arrange.setArmedId(armed?.id ?? null); setSelectedId(selected.id); arrange.setBw(BW);
  const rowActions: WithTrack<TrackActions, Track | ClipTrack> = {
    devLabel: t => devLabel(t), about: t => t.about ? aboutText(t.about) : "", rename: (t, v) => renameTo(t, v),
    pattern: (t, name) => raw(t).pats?.[name] ?? null, autoLane: (t, ln) => addLane(t, ln as never), addAutoLane: t => addAutoLane(t),
    mute: t => toggleMuteSolo(t, false), solo: t => toggleMuteSolo(t, true), arm: t => toggleArm(t), volume: (t, v, done) => setVolume(t, v, done),
    click: (t, e) => headerClick(t, e), key: (t, e, h) => headerKey(t, e, h), menu: (t, e) => headerMenu(t, e), startRename: t => renameTrack(t),
  };
  mountTracks(hdrs, lanes, lw, tracks as unknown as Track[], rowActions as unknown as TrackActions);
  initDevice();
  initTracks(); initFiles();
  mountMixer(el("mixer"), song, mixerActions());
  // A restore rebuilds the views for the restored song and selection.
  initCore({
    restored: () => { rebuildTracks(); selectTrack(selected); updateArmed(); },
    changed: () => { dirtyUpd(); livePush(); },
  });
  initClips({
    fitSong: quiet => fitSong(quiet), renderDevice: () => renderDevice(), selectTrack: t => selectTrack(t),
    clipPicked: (t, c) => { rollClips.set(t.id, c); selectTrack(t); centreRoll(); },
    showRoll: () => { showTab("tab-roll"); centreRoll(); },
    rollChanged: redraw => { genRoll(selected); if (redraw) drawRoll(); },
  });
  lib.setCatalog(daw.library as never);
  if ((daw.library as { gm?: unknown } | null)?.gm && E.bank == null) api.bank().then(b => { if (b) E.setBank(b); });
  genRoll(selected);
  renderAll();
  initTabs();
  // Splitters: drag or arrows, sizes remembered in the browser.
  mountSplitter(el("split-v-slot"), { id: "split-v", label: "Resize browser", orientation: "vertical", cssVar: "--browser-w", min: 140, max: 360,
    measure: () => el("browser").getBoundingClientRect().width, at: e => e.clientX });
  mountSplitter(el("split-h-slot"), { id: "split-h", label: "Resize arrangement", orientation: "horizontal", cssVar: "--arr-h", min: 120, max: 700,
    measure: () => el("arr").getBoundingClientRect().height, at: e => e.clientY - el("work").getBoundingClientRect().top });
  // Engine link: a seek and every playing frame refresh the arrangement, automation and recording.
  initEngine({
    showPos: () => showPos(),
    seeked: () => { applyAutomation(); laneVals.forEach(l => l.val()); },
    frame: now => { recordFrame(); showPos(); applyAutomation(); laneTick(now); },
  });
  relayout(); updateLoop(); showPos();
  queueMicrotask(() => updateArmed());
  // Footer: the armed track's name follows renames through the store.
  mountAudition(el("audition"), { armedName: () => { const id = arrange.armedId(); return id == null ? null : tracks.find(t => t.id === id)?.name ?? null; },
    toggleKb, connectMidi, wheel: setWheel, bend: setBend });
  markClean();
  snap();  // warms the lane-point cache: the first gesture does not serialize the whole song
  daw.debug = { song: () => songOf(false), selected: () => selected.id, bars: () => BARS, undoDepth: () => H.depth, live: () => liveStats() };
  // Global shortcuts: Ctrl/⌘+S and the computer keyboard before the focused editor, the rest after it.
  installKeys({
    save, undo, redo, toggleKb, kbPlays: () => !!(au.kbOn() && armed),
    kbNoteOn: (i, code) => noteOn(armed, 12 * au.kbOct() + i, au.kbVel(), "k" + code), kbNoteOff: code => noteOff("k" + code),
    octave: d => au.setKbOct(au.kbOct() + d), velocity: d => au.setKbVel(au.kbVel() + 16 * d),
    allNotesOff,
    playPause: () => setPlay(!playing), toHome: () => seek(loopOn ? LOOP[0] : 1), toggleLoop, loopSelection, toggleFollow, toggleRec: () => setRec(!recOn),
    inRoll,
    zoomRoll: f => zoomRoll(f, 1), zoomArr: f => zoomArr(f), fitRoll, fitArr,
  });
  mountRoll(el("p-roll"), { attach: attachRoll, detach: detachRoll });  // last: ui/app/roll.ts has bound its mount setups
}
