import { expect, test } from "bun:test";
import type { Audio, Report } from "../synth-parity/metrics.ts";
import { cut, formatRun, passes, type V2Row } from "./v2.ts";

const report = (pass: boolean): Report => ({
  loudness: { ours: -20, ref: -20.1, diff: 0.1, pass },
  bands: [63, 125, 250, 500, 1000, 2000, 4000, 8000, 16000].map((fc) => ({ fc, ours: 10, ref: 10, diff: fc === 500 ? -0.4 : 0.2, pass: true })),
  onsets: { total: 4, matched: 4, extra: 0, maxOffset: 0, pass: true, lag: 0, corr: 0.97 },
  pass,
});

test("rows pass with a passing report and no error", () => {
  expect(passes({ name: "a", report: report(true) })).toBe(true);
  expect(passes({ name: "a", report: report(false) })).toBe(false);
  expect(passes({ name: "a", error: "convert: boom" })).toBe(false);
});

test("the renders are compared over the shorter one", () => {
  const a: Audio = { rate: 44100, ch: [new Float32Array(10), new Float32Array(10)] };
  const b: Audio = { rate: 44100, ch: [new Float32Array(7), new Float32Array(7)] };
  const [x, y] = cut(a, b);
  expect([x.ch[0].length, y.ch[1].length]).toEqual([7, 7]);
});

test("the report lists every song with its worst band and the listening state", () => {
  const rows: V2Row[] = [
    { name: "fr08", version: 0, report: report(true), secs: 660, ourTime: 40 },
    { name: "kkrieger", version: 5, error: "render: boom" },
  ];
  const md = formatRun({ rows, signoff: { decision: "pending" }, generated: "2026-10-28" });
  expect(md).toContain("1 of 2 pass.");
  expect(md).toContain("| fr08 | 0 | PASS | 0.10 | -0.40 @ 500 | 0.97 | 16.5 |");
  expect(md).toContain("| kkrieger | 5 | ERROR |");
  expect(md).toContain("Pending:");
});

test("aux-bus corpus songs get their own section", () => {
  const md = formatRun({ rows: [{ name: "fr08", report: report(true) }, { name: "debris_ost", version: 6, report: report(true) }], signoff: { decision: "pending" }, generated: "2026-10-28" });
  const [main, aux] = md.split("## Aux buses (corpus songs)");
  expect(main).toContain("| fr08 |");
  expect(main).not.toContain("debris_ost");
  expect(aux).toContain("| debris_ost | 6 | PASS |");
});
