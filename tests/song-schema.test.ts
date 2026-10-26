import { expect, test } from "bun:test";
import Ajv2020 from "ajv/dist/2020";
import { readdirSync, readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

// song/1 and us-patch/1 files match their published schemas (change add-synth-daw, task 2.1).
const root = fileURLToPath(new URL("../", import.meta.url));
const load = (p: string) => JSON.parse(readFileSync(root + p, "utf8"));
const ajv = new Ajv2020({ allErrors: true });
const song = ajv.compile(load("schema/song.schema.json"));
const patch = ajv.compile(load("schema/us-patch.schema.json"));

test("example song matches the song schema", () => {
  const ok = song(load("project/music/demo.song.json"));
  expect(song.errors ?? []).toEqual([]);
  expect(ok).toBe(true);
});

test("every song in project/music (converted songs with about and patterns) matches the song schema", () => {
  const files = readdirSync(root + "project/music").filter((f) => f.endsWith(".song.json"));
  for (const f of files) {
    const ok = song(load("project/music/" + f));
    expect([f, song.errors ?? []]).toEqual([f, []]);
    expect(ok).toBe(true);
  }
});

test("example presets match the patch schema", () => {
  const files = readdirSync(root + "project/music/patches").filter((f) => f.endsWith(".patch.json"));
  expect(files.length).toBeGreaterThanOrEqual(2);
  for (const f of files) {
    const ok = patch(load("project/music/patches/" + f));
    expect(patch.errors ?? []).toEqual([]);
    expect(ok).toBe(true);
  }
});

test("song schema rejects unknown fields and bad notes", () => {
  const s = load("project/music/demo.song.json");
  expect(song({ ...s, colour: "red" })).toBe(false);
  const bad = structuredClone(s);
  bad.clips[0].notes[0] = [0, 0, 200, 0];
  expect(song(bad)).toBe(false);
  expect(song({ ...s, format: "song/2" })).toBe(false);
});

test("every song in project/music matches the song schema, and a song with meta says who made it", () => {
  const files = readdirSync(root + "project/music").filter((f) => f.endsWith(".song.json"));
  expect(files.length).toBeGreaterThanOrEqual(1);
  for (const f of files) {
    const s = load("project/music/" + f);
    expect(song(s), f + " " + JSON.stringify(song.errors)).toBe(true);
    if (s.meta?.adapted_from) expect(s.meta.description.toLowerCase(), f).toContain("adapted");
  }
});

test("meta needs author, description, copyright and an http url; unknown keys are rejected", () => {
  const s = load("project/music/demo.song.json");
  const meta = { author: "A", description: "D", copyright: "C", url: "https://example.org" };
  expect(song({ ...s, meta })).toBe(true);
  expect(song({ ...s, meta: { ...meta, adapted_from: "X", license: "MIT", title: "T" } })).toBe(true);
  for (const k of ["author", "description", "copyright", "url"]) {
    const m: Record<string, string> = { ...meta };
    delete m[k];
    expect(song({ ...s, meta: m }), k).toBe(false);
  }
  expect(song({ ...s, meta: { ...meta, url: "javascript:alert(1)" } })).toBe(false);
  expect(song({ ...s, meta: { ...meta, mood: "calm" } })).toBe(false);
});
