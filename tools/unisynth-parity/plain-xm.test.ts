import { expect, test } from "bun:test";
import { mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { engineMissing, formatRun, passes } from "./plain-xm.ts";

test("a missing engine is named from synth-convert's error", () => {
  expect(engineMissing("synth-convert: m.json: engine `fm-shaper` (instrument 1 sample 0) is not available in this build")).toBe("fm-shaper");
  expect(engineMissing("synth-convert: x.xm: effect K at pattern 0, row 1, channel 2")).toBeUndefined();
});

test("pending and skipped rows do not fail a run; a failed one does", () => {
  expect(passes([{ name: "a", status: "pending" }, { name: "b", status: "skip" }, { name: "c", status: "pass" }])).toBe(true);
  expect(passes([{ name: "a", status: "pending" }, { name: "b", status: "fail" }])).toBe(false);
  expect(formatRun([{ name: "quad-theta", status: "pending", note: "engine `poespack` not in this build yet" }], "2026-11-10")).toContain("| quad-theta | pending | – | – | engine `poespack` not in this build yet |");
});

test("absent material skips with exit 0", () => {
  const p = Bun.spawnSync(["bun", join(import.meta.dir, "plain-xm.ts")], { env: { ...process.env, ZENEKK_DIR: mkdtempSync(join(tmpdir(), "zenekk-")) }, stdout: "pipe" });
  expect(p.exitCode).toBe(0);
  expect(p.stdout.toString()).toContain("skip: plain-XM material absent");
});
