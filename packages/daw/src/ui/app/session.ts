// The running DAW session (change daw-solid-ts 9.1, step 2): what main.ts hands over (engine, api, tables, first
// song) and the song store, plus the selected and armed track. Exported as live bindings: importers read the
// current value; only the setters assign (selection is not a signal here: `selectedId` in state/view.ts and
// `armedId` in state/arrange.ts are what components track).
import type { Engine, wav16 } from "../../engine.ts";
import type * as model from "../../model.ts";
import type { api as Api } from "../../api.ts";
import type { FileEntry } from "../../api.ts";
import type { Table } from "../../engine.ts";
import type { UITrack } from "../../model.ts";
import { createSong, type SongRoot } from "../state/song.ts";
import { instOf } from "../model/track.ts";

export interface Daw {
  engine: Engine; model: typeof model; api: typeof Api; tables: Table[]; initial: model.UIState; files: FileEntry[];
  file: string; library: unknown; wav16: typeof wav16;
  debug?: Record<string, unknown>;
}

export let DAW: Daw;
export let E: Engine;
export let api: typeof Api;
/** The song store (ui/state/song.ts) and its proxies; loads and undo change them in place, never replace them. */
export let song: SongRoot;
export let tracks: UITrack[];
export let master: UITrack;
/** The track the editors show (always a store proxy), and the one the computer keyboard and MIDI play. */
export let selected: UITrack;
export let armed: UITrack | null;

export function initSession(daw: Daw): void {
  DAW = daw; E = daw.engine; api = daw.api;
  song = createSong(daw.initial); tracks = song.tracks; master = song.master;
  selected = tracks.find((t) => !t.bus) || tracks[0] || master;
  armed = tracks.find((t) => instOf(t)) || null;
}
export function setSelected(t: UITrack): void { selected = t; }
export function setArmed(t: UITrack | null): void { armed = t; }
