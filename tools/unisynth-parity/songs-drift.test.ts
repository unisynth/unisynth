import { expect, test } from "bun:test";
import { items, patchSong } from "./songs-drift.ts";

test("every tracked song and patch is an item, with unique slugs", () => {
  const all = items();
  expect(all.filter((i) => i.file.endsWith(".patch.json")).length).toBe(2);
  expect(all.filter((i) => i.file.startsWith("project/music/released/v2/")).length).toBe(438);
  expect(all.filter((i) => i.slug.startsWith("template-")).length).toBe(2);
  expect(new Set(all.map((i) => i.slug)).size).toBe(all.length);
});

test("a patch plays as a one-note song", () => {
  const s = JSON.parse(patchSong({ kind: 64, values: { "osc1.type": 0.25 } }));
  expect(s.tracks[0].devices[0]).toEqual({ kind: 64, values: { "osc1.type": 0.25 } });
  expect(s.clips[0].notes).toEqual([[0, 3840, 60, 100]]);
});
