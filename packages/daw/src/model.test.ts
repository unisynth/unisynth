// song/1 ⇄ editor model adapter: the demo song survives fromSong → toSong with the same placements, notes,
// devices, routing and automation, and the result validates in the engine module (crates/song rules).
import { expect, test } from "bun:test";
import { readdirSync, readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { join } from "node:path";
import { audible, fromSong, toSong, type Song } from "./model.ts";
import { loadSynth } from "../../cli/src/synth.ts";

const ROOT = fileURLToPath(new URL("../../../", import.meta.url));
const demo = (): Song => JSON.parse(readFileSync(join(ROOT, "project/music/demo.song.json"), "utf8"));

test("demo song round-trips through the editor model and still validates", () => {
  const s = demo();
  const ui = fromSong(s);
  expect(ui.tracks.map((t) => t.name)).toEqual(["Kick", "Bass", "Lead", "Pad", "Verb"]);
  expect(ui.tracks.find((t) => t.name === "Verb")!.bus).toBe(true);
  expect(ui.tracks.find((t) => t.name === "Lead")!.sends.map((x) => [x[0], x[1], Math.round(x[2] * 10) / 10])).toEqual([["Verb", "main", -8]]);
  const back = toSong(ui);
  expect(loadSynth().validateSong(JSON.stringify(back))).toEqual([]);
  // same notes per placement (clip ids are track-scoped in the editor)
  const notes = (x: Song, track: string) => x.tracks.find((t) => t.id === track)!.placements.map((p) => [p.start, p.length, p.loop, x.clips.find((c) => c.id === p.clip)!.notes]);
  for (const id of ["kick", "bass", "lead", "pad"]) expect(notes(back, id)).toEqual(notes(s, id));
  expect(back.tracks.map((t) => t.devices)).toEqual(s.tracks.map((t) => t.devices));
  expect(back.tracks[1].lanes).toEqual(s.tracks[1].lanes);
  const rc = (x: Song) => x.tracks.map((t) => t.receives.map((r) => [r.from, r.channel, r.volume]));
  expect(rc(back)).toEqual(rc(s));
  expect(back.tracks.map((t) => t.volume)).toEqual(s.tracks.map((t) => t.volume));
  expect(back.master.devices).toEqual(s.master.devices);
  expect(back.master.receives.map((r) => r.from)).toEqual(["kick", "bass", "lead", "pad", "verb"]);
  expect(back.length).toBe(s.length);
});

test("solo for live preview mutes the others without changing the source", () => {
  const ui = fromSong(demo());
  const s = toSong(ui, (t) => t.name === "Bass");
  expect(s.tracks.map((t) => t.mute)).toEqual([true, false, true, true, true]);
  expect(toSong(ui).tracks.every((t) => !t.mute)).toBe(true);
});

test("solo keeps the send sources of a soloed track and the buses it sends to", () => {
  const ui = fromSong(demo());
  const [kick, bass, lead, pad] = ui.tracks;
  // kick feeds the pad's sidechain; pad sends to the verb bus
  kick.sends = [["Pad", "side", 0]];
  pad.solo = true;
  pad.sends = [["Verb", "main", -6]];
  const on = (t: (typeof ui.tracks)[number]) => audible(ui.tracks, t);
  expect(ui.tracks.map(on)).toEqual([true, false, false, true, true]);
  // muted wins over solo membership; nothing soloed = everything not muted
  kick.mute = true;
  expect(on(kick)).toBe(false);
  pad.solo = false;
  expect([bass, lead].map(on)).toEqual([true, true]);
  expect(toSong(ui, on).tracks.map((t) => t.mute)).toEqual([true, false, false, false, false]);
});

test("an edited volume is written from its dB value", () => {
  const ui = fromSong(demo());
  ui.tracks[0].vol = -6;
  expect(toSong(ui).tracks[0].volume).toBeCloseTo(0.501187, 6);
});

test("clip ids and colours survive a load and save", () => {
  const s = demo(), back = toSong(fromSong(s));
  expect(back.clips.map((c) => c.id)).toEqual(s.clips.map((c) => c.id));
  expect(back.tracks.map((t) => t.color)).toEqual(s.tracks.map((t) => t.color));
  expect(back.master.color).toBeUndefined();
  expect(JSON.parse(JSON.stringify(back))).toEqual(s);
});

test("song meta survives a load and save; flat controller envelopes are not written", () => {
  const s: Song = { ...demo(), meta: { title: "Demo", licence: "MIT" } } as Song;
  const ui = fromSong(s);
  expect(toSong(ui).meta).toEqual({ title: "Demo", licence: "MIT" });
  expect(Object.keys(toSong(ui))[1]).toBe("meta");
  ui.tracks[1].pats![Object.keys(ui.tracks[1].pats!)[0]].cc = { wheel: [[0, 0]], bend: [[0, 0.5]] };
  expect(toSong(ui).clips.find((c) => c.id === "bass-a")!.wheel).toBeUndefined();
  ui.tracks[1].pats![Object.keys(ui.tracks[1].pats!)[0]].cc = { wheel: [[0, 0], [8, 0.7]], bend: [] };
  expect(toSong(ui).clips.find((c) => c.id === "bass-a")!.wheel).toEqual([[0, 0], [1920, 0.7]]);
});

test("speech texts survive a load and save", () => {
  const s = demo();
  const dev = s.tracks.flatMap((t) => t.devices).find((d) => d.kind === 64)!;
  (dev as { speech?: string[] }).speech = ["AH27OH40", "IH22UH11"];
  const back = toSong(fromSong(s));
  expect(back.tracks.flatMap((t) => t.devices).find((d) => d.kind === 64)).toEqual(dev);
});

test("converted reference songs in project/music load and save through the adapter unchanged", () => {
  const dir = join(ROOT, "project/music") + "/";
  for (const f of readdirSync(dir).filter((x: string) => x.endsWith(".song.json"))) {
    const s = JSON.parse(readFileSync(dir + f, "utf8"));
    expect(JSON.parse(JSON.stringify(toSong(fromSong(s))))).toEqual(s);
  }
});
