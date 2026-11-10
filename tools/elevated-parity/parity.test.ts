import { expect, test } from "bun:test";
import { mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { DEFAULT_TOL } from "../synth-parity/metrics.ts";
import { available, cut, formatReport, TOL } from "./parity.ts";

test("tolerances are the song parity's: the defaults, the band floor and onset alignment", () => {
  expect(TOL).toEqual({ ...DEFAULT_TOL, bandFloorDb: 24, onsetAlignment: 0.5 });
});

test("songs without sources or reference renders are skipped", () => {
  const empty = mkdtempSync(join(tmpdir(), "elev-"));
  process.env.ELEVATED_SRC = empty;
  process.env.ELEVATED_REF = empty;
  expect(available(["atrium", "elevated"])).toEqual({ run: [], skipped: ["atrium", "elevated"] });
  expect(formatReport([], ["atrium"])).toContain("Skipped (sources or reference render absent): atrium.");
});

test("the skip path exits 0", () => {
  const empty = mkdtempSync(join(tmpdir(), "elev-"));
  const p = Bun.spawnSync(["bun", join(import.meta.dir, "parity.ts")], { env: { ...process.env, ELEVATED_SRC: empty, ELEVATED_REF: empty }, stdout: "pipe" });
  expect(p.exitCode).toBe(0);
  expect(p.stdout.toString()).toContain("elevated parity: skipped");
});

test("renders are cut to the shorter one", () => {
  const a = { rate: 44100, ch: [new Float32Array(10), new Float32Array(10)] };
  const b = { rate: 44100, ch: [new Float32Array(7), new Float32Array(7)] };
  const [x, y] = cut(a, b);
  expect([x.ch[0].length, y.ch[1].length]).toEqual([7, 7]);
});
