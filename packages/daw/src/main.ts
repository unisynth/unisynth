// DAW entry (change add-synth-daw): compiles the engine module, reads the parameter tables, the song list, the
// library catalog and the first song, then starts the UI (app.js). Song: `?song=project/music/x.song.json`.
import { Engine, wav16 } from "./engine.ts";
import * as model from "./model.ts";
import { api } from "./api.ts";
import { start } from "./app.js";

async function boot() {
  const engine = await Engine.create(new URL(".", location.href).href);
  const tables = engine.tables();
  const files = await api.list().catch(() => []);
  const want = new URLSearchParams(location.search).get("song");
  const songs = files.filter((f) => f.kind === "song").map((f) => f.path);
  const file = want ?? (songs.includes("project/music/demo.song.json") ? "project/music/demo.song.json" : songs[0]);
  let initial: model.UIState;
  try {
    initial = model.fromSong(JSON.parse(await api.read(file!)));
  } catch {
    initial = model.fromSong({
      format: "song/1", rate: 44100, tempo: 120, signature: [4, 4], ticks_per_beat: 960, length: 8 * 3840, clips: [],
      tracks: [{ id: "t1", name: "Track 1", color: "--t-lead", volume: 0.5, mute: false, receives: [], devices: [{ kind: 64, preset: "init", values: { "osc1.type": 0.5 } }], placements: [], lanes: [] }],
      master: { id: "master", name: "Master", volume: 1, mute: false, receives: [{ from: "t1", channel: "main", volume: 1 }], devices: [], placements: [], lanes: [] },
    });
  }
  const library = await api.library().catch(() => null);
  const daw = { engine, model, api, tables, initial, files, file: file ?? "project/music/untitled-1.song.json", library, wav16 };
  (window as any).DAW = daw;
  start(daw);
}

boot().catch((e) => {
  document.body.insertAdjacentHTML("afterbegin", `<div role="alert" style="padding:8px;background:#5a1d1d;color:#fff">DAW failed to start: ${String(e?.message ?? e)}</div>`);
  console.error(e);
});
