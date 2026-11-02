// Mixer actions of the running session (change daw-solid-ts 9.1, step 9): each strip = inserts (the track's effect
// chain), sends (stored as receives on the target track, post-fader), fader, meters. Bus tracks have no instrument;
// the master is the last track and can hold effects. Strips name tracks by id (ui/mixer.tsx).
import type { UITrack } from "../../model.ts";
import type { MixerActions } from "../mixer-store.ts";
import { devName, effectChoices } from "../model/params.ts";
import { master, tracks } from "./session.ts";
import { commit, snap } from "./core.ts";
import { livePush } from "./engine.ts";
import { addEffect, devBypass, renderAll } from "./device.ts";
import { addTrack, selectTrack, setVolume, toggleMuteSolo } from "./tracks.ts";
import { showTab } from "./tabs.ts";

const trackById = (id: string): UITrack => id === master.id ? master : tracks.find(x => x.id === id)!;
let sendBefore: string | null = null;  // undo snapshot of a send-level gesture
/** The mixer's actions (an insert's name opens the strip's track in the Device tab). */
export function mixerActions(): MixerActions {
  return {
    select: id => selectTrack(trackById(id)),
    mute: id => toggleMuteSolo(trackById(id), false), solo: id => toggleMuteSolo(trackById(id), true),
    volume: (id, v, done) => setVolume(trackById(id), v, done),
    openInsert: id => { selectTrack(trackById(id)); showTab("tab-dev"); },
    bypass: (id, i) => devBypass(trackById(id), i),
    effects: () => effectChoices().map(tb => tb.label),
    addInsert: (id, label) => addEffect(trackById(id), label),
    // targets: any track except itself, the master, and tracks that already feed this one (would loop)
    sendTargets: id => {
      const t = trackById(id), feeds = new Set([t.name]); let grew = true;
      while (grew) { grew = false; for (const x of tracks) if (!feeds.has(x.name) && x.sends.some(s1 => feeds.has(s1[0]))) { feeds.add(x.name); grew = true; } }
      return tracks.filter(x => !feeds.has(x.name)).flatMap((x): [string, "main" | "side"][] => [[x.name, "main"], [x.name, "side"]]);
    },
    addSend: (id, to, ch) => { const t = trackById(id), before = snap(); t.sends.push([to, ch, -12]); renderAll(); commit(before, `send ${t.name} → ${to} (${ch === "side" ? "sidechain" : "main"}, post-fader)`); },
    removeSend: (id, i) => { const t = trackById(id), before = snap(), [to] = t.sends[i]!; t.sends.splice(i, 1); renderAll(); commit(before, `send ${t.name} → ${to} removed`); },
    sendLevel: (id, i, db, done) => {
      const t = trackById(id), sd = t.sends[i]!;
      if (done) { if (sendBefore) commit(sendBefore, `send ${t.name} level ${db.toFixed(1)} dB`); sendBefore = null; return; }
      sendBefore ??= snap(); sd[2] = db; sd[3] = undefined; livePush();
    },
    addBus: () => addTrack(true),
    devName,
  };
}
