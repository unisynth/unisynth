// Undo history and song restore (change daw-solid-ts 2.5, design D2). Real Solid runtime via the test preload.
import { expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import { fromSong, toSong } from "../../model.ts";
import { createHistory } from "./history.ts";
import { createSong, raw, restoreSong } from "./song.ts";

const DEMO = JSON.parse(readFileSync(new URL("../../../../../project/music/demo.song.json", import.meta.url), "utf8"));
const setup = () => {
  const st = fromSong(DEMO), song = createSong(st);
  const snap = () => JSON.stringify({ tracks: raw(song.tracks), master: raw(song.master) });
  const exported = () => JSON.stringify(toSong({ ...st, tracks: raw(song.tracks), master: raw(song.master) }));
  return { song, snap, exported, h: createHistory() };
};

test("commit skips an unchanged gesture; undo and redo return the exact song", () => {
  const { song, snap, exported, h } = setup();
  const saved = exported();
  let before = snap();
  expect(h.commit(before, snap())).toBe(false);
  song.tracks[0].vol -= 3; song.tracks[1].clips.pop();
  expect(h.commit(before, snap())).toBe(true);
  const edited = exported();
  restoreSong(song, JSON.parse(h.undo(snap())!));
  expect(exported()).toBe(saved);
  restoreSong(song, JSON.parse(h.redo(snap())!));
  expect(exported()).toBe(edited);
  expect(h.undo(snap())).not.toBeNull();
  expect(h.undo(snap())).toBeNull();          // nothing left
});

test("restore keeps the identity of tracks and devices; reorders by track id", () => {
  const { song, snap, h } = setup();
  const t1 = song.tracks[1], vals = song.tracks[1].devices[0].values;
  const before = snap();
  song.tracks.splice(0, 2, song.tracks[1], song.tracks[0]);   // move track 2 up
  song.tracks[2].vol = -1;
  h.commit(before, snap());
  restoreSong(song, JSON.parse(h.undo(snap())!));
  expect(song.tracks[1]).toBe(t1);
  expect(song.tracks[1].devices[0].values).toBe(vals);
  expect(raw(song.tracks).map((t) => t.id)).toEqual(fromSong(DEMO).tracks.map((t) => t.id));
});

test("restore touches only tracks and master", () => {
  const { song } = setup();
  (song as any).meta = { keep: 1 };
  restoreSong(song, { tracks: raw(song.tracks), master: { ...raw(song.master), vol: -6 } });
  expect((song as any).meta).toEqual({ keep: 1 });
  expect(song.master.vol).toBe(-6);
});

test("state ids: undo back to the saved state is clean, a new edit after undo is not", () => {
  const { song, snap, h } = setup();
  const saved = h.stateId;
  const b = snap(); song.tracks[0].vol = -9; h.commit(b, snap());
  expect(h.stateId).not.toBe(saved);
  restoreSong(song, JSON.parse(h.undo(snap())!));
  expect(h.stateId).toBe(saved);
  const b2 = snap(); song.tracks[0].vol = -8; h.commit(b2, snap());
  expect(h.stateId).not.toBe(saved);
  expect(h.depth).toBe(1);
  h.clear();
  expect([h.depth, h.canRedo]).toEqual([0, false]);
});
