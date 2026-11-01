// Library state (change daw-solid-ts 4.2): catalogue → items per source, search, inspector facts.
import { expect, test } from "bun:test";
import { describe, filterItems, libItems, openGroups, SOURCES, visibleSources } from "./library.ts";

const CAT = {
  project: { presets: [{ name: "Acid Bass", path: "project/music/patches/acid-bass.patch.json" }] },
  gm: { instruments: [{ name: "Piano", drums: false, bank: 0, prog: 1, regions: [[0, 127, 3]] }, { name: "Std Kit", drums: true, bank: 0, prog: 0, regions: [] }] },
  tunefish: { programs: [{ name: "Lead", n: 7, conv: { devices: [{ kind: 64 }, { kind: 68 }, { kind: 69 }], warnings: ["a", "b", "c"] } }, { name: "Raw", n: 8 }] },
};
const src = (id: string) => SOURCES.find((s) => s.id === id)!;

test("items per source, with tags as the tree shows them", () => {
  expect(libItems(null, "project")).toEqual([]);
  expect(libItems(CAT, "gm").map((i) => [i.name, i.tag])).toEqual([["Piano", "001"], ["Std Kit", "kit"]]);
  expect(libItems(CAT, "tunefish").map((i) => i.tag)).toEqual(["#7", "#8"]);
});

test("sources shown: all while loading, then project + those in the catalogue", () => {
  expect(visibleSources(null).length).toBe(SOURCES.length);
  expect(visibleSources(CAT).map((s) => s.id)).toEqual(["project", "gm", "tunefish"]);
});

test("search matches name or tag, case-insensitive; groups open when they have hits or were asked for", () => {
  const items = libItems(CAT, "gm");
  expect(filterItems(items, "KIT").map((i) => i.name)).toEqual(["Std Kit"]);
  expect(filterItems(items, "").length).toBe(2);
  expect(openGroups(CAT, "piano", null)).toEqual({ project: false, gm: true, tunefish: false });
  expect(openGroups(CAT, "", "tunefish")).toEqual({ project: false, gm: false, tunefish: true });
});

test("inspector facts: loadable, legacy hint, conversion note", () => {
  const lead = libItems(CAT, "tunefish")[0], raw = libItems(CAT, "tunefish")[1];
  expect(describe(src("tunefish"), lead, true)).toMatchObject({ canLoad: true, legacy: false, conv: "Converted by synth-convert: instrument + 2 insert effects · 3 notes: a; b" });
  expect(describe(src("tunefish"), raw, true)).toMatchObject({ canLoad: false, legacy: true });
  expect(describe(src("tunefish"), lead, false).canLoad).toBe(false);  // no instrument on the track
  expect(describe(src("gm"), libItems(CAT, "gm")[0], true).what).toBe("melodic · bank 0 · prog 1 · 1 regions");
});
