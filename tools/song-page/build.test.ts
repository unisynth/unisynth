import { expect, test } from "bun:test";
import { relative } from "node:path";
import { fileURLToPath } from "node:url";
import { GROUPS, groupOf, page, slugOf, songFiles } from "./build.ts";

const MUSIC = fileURLToPath(new URL("../../project/music/", import.meta.url));

test("every song in project/music gets its own slug and a known group", () => {
  const rels = songFiles().map((f) => relative(MUSIC, f));
  expect(rels.length).toBeGreaterThan(0);
  const slugs = rels.map(slugOf);
  expect(new Set(slugs).size).toBe(slugs.length);
  for (const s of slugs) expect(s).toMatch(/^[a-z0-9-]+$/);
  for (const r of rels) expect(GROUPS).toContain(groupOf(r));
});

test("groups follow the folder and the source", () => {
  expect(groupOf("demo.song.json")).toBe("Demo songs");
  expect(groupOf("released/atlas.song.json")).toBe("64K intros and executable music (WaveSabre)");
  expect(groupOf("released/tbl-jizz-smokeout.song.json")).toBe("The Black Lotus, 1997");
  expect(groupOf("released/v2/v2-fr08.song.json", "https://github.com/spheenik/v2redux")).toBe("Farbrausch productions");
  expect(groupOf("released/v2/dafunk-breeze.song.json", "https://github.com/jgilje/v2m-player/blob/master/v2m/x")).toBe("V2M collection (via modland)");
  expect(slugOf("released/v2/v2-fr08.song.json")).toBe("v2-v2-fr08");
});

test("the page escapes song text and keeps the script closed", () => {
  const e = { slug: "x", file: "x.song.json", group: "Demo songs", title: "</script><b>", author: "a&b", description: "d", seconds: 61, audio: "audio/x.m4a", bytes: 1 };
  const html = page([e]);
  expect(html.match(/<\/script>/g)?.length).toBe(1);
  expect(html).toContain("\\u003c/script>");
});
