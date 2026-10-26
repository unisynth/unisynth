import { expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import { meta, type Manifest } from "./build";

const manifest: Manifest = JSON.parse(readFileSync(new URL("./songs.json", import.meta.url), "utf8"));

test("every demo song has a unique name, a known source and the right file type for its converter", () => {
  const names = manifest.songs.map((s) => s.name);
  expect(new Set(names).size).toBe(names.length);
  expect(names.length).toBe(7);
  for (const s of manifest.songs) {
    const src = manifest.sources[s.source];
    expect(src, s.name).toBeDefined();
    expect(s.file.endsWith({ tfm: ".tfm", tf4m: ".tf4m", ws: ".bin" }[src.convert]), s.name).toBe(true);
    expect(s.name).toMatch(/^[a-z0-9-]+$/);
  }
});

test("the meta credits the original, names the license of its code and says the work is adapted", () => {
  for (const e of manifest.songs) {
    const m = meta(manifest, e);
    const s = manifest.sources[e.source];
    expect(m.author.length, e.name).toBeGreaterThan(0);
    expect(m.copyright).toBe(s.copyright);
    expect(m.license).toBe({ tunefish: "GPL-3.0-or-later", "enigma-studio-4": "CC0-1.0", wavesabre: "MIT" }[e.source]!);
    expect(m.url.startsWith("https://github.com/"), e.name).toBe(true);
    expect(m.adapted_from).toContain(m.url);
    expect(m.description.toLowerCase()).toContain("adapted");
    expect(m.description).toContain("not the original");
    expect(m.description).toContain(s.pipeline);
  }
});

test("the url points at the original file, escaped", () => {
  const g = manifest.songs.find((s) => s.name === "global-groove")!;
  expect(meta(manifest, g).url.endsWith("/Global%20Groove.xrns")).toBe(true);
});

test("no composer is invented: authors the repositories do not name say so", () => {
  for (const e of manifest.songs) if (e.name !== "eladamri") expect(e.author, e.name).toMatch(/not named|WaveSabre Team$/);
});
