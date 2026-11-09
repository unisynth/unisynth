import { describe, expect, test } from "bun:test";
import { spawnSync } from "node:child_process";
import { join } from "node:path";
import { readProject } from "../tf3-parity/e3prj.ts";
import { formatRun, itemProject, ITEMS, passes, type Tf3Run } from "./tf3.ts";

const report = (r: Partial<Tf3Run>): string => formatRun({ rows: [], generated: "2026-11-09", ...r });

describe("tf3 parity", () => {
  test("every synthetic item is a readable one-instrument project that plays", () => {
    for (const name of Object.keys(ITEMS)) {
      const p = readProject(itemProject(name));
      expect(p.instruments.length).toBe(1);
      const s = p.songs[0];
      const notes = s.patterns[0].tracks[0].filter((c) => c.note > 0 && c.note < 128);
      expect(notes.length).toBeGreaterThan(0);
      expect(notes.every((c) => c.row < s.patterns[0].rows)).toBe(true);
    }
  });

  test("the report names failures, accepted tolerances and the capture, and holds no audio", () => {
    const rep = { loudness: { ours: -12, ref: -12.1, diff: 0.1, pass: true }, bands: [{ fc: 1000, ours: -20, ref: -20.5, diff: 0.5, pass: true }], onsets: { total: 4, matched: 4, extra: 0, maxOffset: 0, pass: true, lag: 0, corr: 0.9 }, pass: true };
    const text = report({
      rows: [
        { name: "a", report: rep },
        { name: "pad-gauss", report: rep, accepted: "loudness ±2 dB" },
        { name: "b", error: "convert: boom" },
      ],
      capture: { skipped: "no capture" },
    });
    expect(text).toContain("| pad-gauss | pass (accepted) |");
    expect(text).toContain("2 of 3 items pass; failing: b");
    expect(text).toContain("Skipped: no capture");
    expect(text).not.toMatch(/RIFF|data:audio/);
    expect(passes({ name: "b", error: "x" })).toBe(false);
  });

  test("skips without the checkout", () => {
    const r = spawnSync("bun", [join(import.meta.dir, "tf3.ts"), "points-step"], { env: { ...process.env, TF3_REF: "/nonexistent/tf3" }, encoding: "utf8" });
    expect(r.status).toBe(0);
    expect(r.stdout).toContain("tf3 parity: skipped");
  });

  test("no Enigma Studio 3 material is tracked (requirement \"Tunefish 3 material stays local\")", () => {
    const git = (...a: string[]) => spawnSync("git", a, { cwd: join(import.meta.dir, "../.."), encoding: "utf8" });
    // projects, presets, songs converted from them, renders and captures
    const files = git("ls-files").stdout.split("\n");
    // (the adapted released songs, project/music/released/*.song.json, follow NOTICE.md's rule and are allowed)
    const material = files.filter((f) => /\.e3prj$|tf3programs\/|(^|\/)program\d+\.txt$|(transplant|pandora|train-control)[^/]*\.(wav|mp3|song|img|exe)$/i.test(f));
    expect(material).toEqual([]);
    // code: names that only the original's implementation bodies use (the spline solve, the PADsynth algorithm, the
    // decimation mix) and its source banner; our harnesses call the original's API but copy no body
    for (const token of ["_solveLinear", "extendedAlgorithm", "profileDetune", "eDownsampleMix", "t i n y   m u s i c"]) {
      const hits = git("grep", "-l", "-F", token, "--", ".", ":!openspec", ":!tools/unisynth-parity/tf3.test.ts").stdout.trim();
      expect(hits).toBe("");
    }
  });

  test("a skipped run says why", () => {
    expect(report({ skipped: "no reference renderer" })).toContain("Skipped: no reference renderer.");
  });
});
