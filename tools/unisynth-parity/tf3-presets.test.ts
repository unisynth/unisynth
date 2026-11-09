import { expect, test } from "bun:test";
import { mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { readProject } from "../tf3-parity/e3prj.ts";
import { format, presets, project } from "./tf3-presets.ts";

test("distinct presets only, lowest number first; a project plays 16 notes with the preset's values", () => {
  const dir = mkdtempSync(join(tmpdir(), "tf3-bank-"));
  writeFileSync(join(dir, "program3.txt"), "Init\r\nOscVol;0.5\r\nOscPoints;0.33\r\n");
  writeFileSync(join(dir, "program10.txt"), "Init copy\nOscVol;0.5\nOscPoints;0.33\n");
  writeFileSync(join(dir, "program2.txt"), "Bass\nOscVol;0.7\nOscPoints;0.33\n");
  const ps = presets(dir);
  expect(ps.map((p) => [p.n, p.name])).toEqual([[2, "Bass"], [3, "Init"]]);
  const prj = readProject(project(ps[0]!.params));
  expect(prj.instruments.length).toBe(1);
  expect(prj.songs[0]!.patterns[0]!.tracks[0]!.filter((c) => c.note > 0 && c.note < 128).length).toBe(16);
});

test("the report counts passes and holds no audio", () => {
  const text = format([{ n: 1, name: "A", loud: 0.2, band: { fc: 500, diff: 1 }, pass: true }, { n: 2, name: "B", loud: -1.2, band: { fc: 63, diff: -5 }, pass: false }, { n: 3, name: "C", error: "boom" }], "2026-11-09");
  expect(text).toContain("1 of 3 within both; every preset within 1.20 dB loudness");
  expect(text).toContain("| 3 | C | error: boom |");
  expect(text).not.toMatch(/RIFF|data:audio/);
});
