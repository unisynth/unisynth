import { expect, test } from "bun:test";
import { relative } from "node:path";
import { fileURLToPath } from "node:url";
import { readFileSync } from "node:fs";
import { GROUPS, groupOf, needsBank, page, sizeOf, slugOf, songFiles } from "./build.ts";

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

test("exactly the songs with a GM.DLS sampler need the bank", () => {
  const need = songFiles().filter((f) => needsBank(JSON.parse(readFileSync(f, "utf8")))).map((f) => relative(MUSIC, f));
  // the WaveSabre songs with Adultery instruments (17 when this was written)
  expect(need).toContain("engage.song.json");
  expect(need).toContain("released/atlas.song.json");
  expect(need).not.toContain("demo.song.json");
  expect(need.every((r) => !r.startsWith("released/v2/"))).toBe(true);
  const osc = (s7: number, sample: number) => ({ tracks: [{ devices: [{ values: { "osc2.type": 1 / 3, "osc2.s7": s7, "osc2.sample": sample } }] }] });
  expect(needsBank(osc(1, 5))).toBe(true);
  expect(needsBank(osc(0.5, 5))).toBe(true); // DLS instrument
  expect(needsBank(osc(0, 5))).toBe(false); // GSM sample of the chunk
  expect(needsBank(osc(1, 0))).toBe(false); // no sample
});

const entry = { slug: "x", file: "x.song.json", group: "Demo songs", title: "</script><b>", author: "a&b", description: "d", seconds: 61, blob: "songs/x.bin", bytes: 1, bank: false, tracks: 3, devices: 7 };

test("the page escapes song text and keeps the script closed", () => {
  const html = page([entry]);
  expect(html.match(/<\/script>/g)?.length).toBe(1);
  expect(html).toContain("\\u003c/script>");
});

test("the page plays song files with the synth, not audio files", () => {
  const html = page([entry], "../daw/data/bank.dls");
  expect(html).not.toContain("<audio");
  expect(html).toContain('"songs/x.bin"');
  expect(html).toContain('"../daw/data/bank.dls"');
  expect(html).toContain("synth.wasm");
  expect(html).toContain("registerProcessor");
  expect(html).toContain("bounce_open");
});

test("a song's size counts its tracks and devices", () => {
  expect(sizeOf({ tracks: [{ devices: [{}, {}] }, { devices: [{}] }, {}] })).toEqual({ tracks: 3, devices: 3 });
  expect(sizeOf({})).toEqual({ tracks: 0, devices: 0 });
});

test("the page is the listening app: deck with a seek canvas, a filterable list and a detail panel", () => {
  const html = page([entry, { ...entry, slug: "y", group: "The Black Lotus, 1997" }]);
  for (const id of ['id="deck"', 'class="seek"', "<canvas", 'id="q"', 'id="chips"', 'id="sort"', 'id="list"', 'id="detail"', 'class="loop"'])
    expect(html).toContain(id);
  expect(html).toContain("createAnalyser"); // live spectrum in the seek bar
  expect(html).toContain('"tracks":3'); // stat cards come from the entries
  expect(html).toContain('href="../"'); // back to the site
});

test("the page's inline script is valid JavaScript and its description links survive escaping", () => {
  const html = page([{ ...entry, description: "see `synth-mix` (http://a.b/c?d=1)." }]);
  const code = html.slice(html.indexOf("<script>") + 8, html.indexOf("</script>"));
  expect(() => new Function(code)).not.toThrow(); // a template-literal escape slip (\s → s) breaks the whole page
  expect(code).toContain("https?:\\/\\/[^\\s)<]");
});
