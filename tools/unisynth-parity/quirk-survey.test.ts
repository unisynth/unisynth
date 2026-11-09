import { expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { QUIRKS, versionOf } from "./quirk-survey.ts";

test("every V2 song states its format version, and each quirk covers some version", () => {
  const dir = fileURLToPath(new URL("../../project/music/released/v2/", import.meta.url));
  const vs = [...new Bun.Glob("*.song.json").scanSync(dir)].map((f) => versionOf(JSON.parse(readFileSync(join(dir, f), "utf8")).meta.description));
  expect(vs.length).toBe(358);
  expect(vs.every((v) => v !== undefined && v >= 0 && v <= 6)).toBe(true);
  for (const q of QUIRKS) expect([0, 1, 2, 3, 4, 5, 6].some(q.sets)).toBe(true);
  expect(versionOf("a Farbrausch V2 song (.v2m format version 5) taken from")).toBe(5);
});
