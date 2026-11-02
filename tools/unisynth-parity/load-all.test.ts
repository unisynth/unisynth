// Load-all check (change unify-v2-algorithms 6.9): a current song loads, an old-format file or a song the player
// refuses fails naming why, and the inventory covers every tracked location of the design's table.
import { expect, test } from "bun:test";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { fileURLToPath } from "node:url";
import { check, inventory } from "./load-all.ts";

const ROOT = fileURLToPath(new URL("../../", import.meta.url));
const demo = readFileSync(join(ROOT, "project/music/demo.song.json"), "utf8");

test("a song/2 file loads; a song/1 file and a song the player refuses fail naming why", async () => {
  const tmp = mkdtempSync(join(tmpdir(), "load-all-test-"));
  try {
    expect(await check({ name: "demo", file: join(ROOT, "project/music/demo.song.json"), format: "song/2" }, tmp, 0)).toEqual({ name: "demo", ok: true });
    const oldFile = join(tmp, "old.song.json");
    writeFileSync(oldFile, demo.replace('"format": "song/2"', '"format": "song/1"'));
    const old = await check({ name: "old", file: oldFile, format: "song/2" }, tmp, 1);
    expect(old.ok).toBe(false);
    expect(old.why).toContain("format song/1, not song/2"); // it would load (migrated on parse), but is not migrated
    const bad = JSON.parse(demo);
    bad.tracks[0].devices[0].values["osc1.type"] = 7; // out of range: the song does not validate
    const r = await check({ name: "bad", json: JSON.stringify(bad) }, tmp, 2);
    expect(r.ok).toBe(false);
    expect(r.why).toContain("osc1.type");
    expect(await check({ name: "lib", bad: "format us-library/1, not us-library/2 (rebuild it)" }, tmp, 3)).toMatchObject({ ok: false, why: expect.stringContaining("us-library/1") });
  } finally {
    rmSync(tmp, { recursive: true, force: true });
  }
});

test("the inventory lists the tracked locations with their recorded counts", () => {
  const tracked = inventory(false).filter((l) => l.kind === "tracked").map((l) => [l.name, l.recorded]);
  expect(tracked).toEqual([
    ["project/music/*.song.json", 8],
    ["project/music/patches/*.patch.json", 2],
    ["project/music/released/*.song.json", 28],
    ["project/music/released/v2/*.song.json", 347],
    ["DAW instrument templates (app.js, main.ts)", 2],
  ]);
});
