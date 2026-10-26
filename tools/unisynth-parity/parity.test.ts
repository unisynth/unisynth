import { expect, test } from "bun:test";
import { mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import type { Report } from "../synth-parity/metrics.ts";
import { ACCEPTED, formatRun, passes, realTime, referenceSongs, retirement, SEEDS, type Row, type Run } from "./parity.ts";

const report = (pass: boolean, diff = 0.1): Report => ({
  loudness: { ours: -20, ref: -20 - diff, diff, pass },
  bands: [63, 125, 250, 500, 1000, 2000, 4000, 8000, 16000].map((fc) => ({ fc, ours: 10, ref: 10, diff: 0.2, pass: true })),
  onsets: { total: 4, matched: 4, extra: 0, maxOffset: 0, pass: true, lag: 0, corr: 0.97 },
  pass,
});
const row = (name: string, level: Row["level"], pass = true, extra: Partial<Row> = {}): Row => ({ level, name, engine: "ws", report: report(pass), ...extra });
const run = (rows: Row[], over: Partial<Run> = {}): Run => ({ rows, signoff: { decision: "approved", by: "robson", date: "2026-10-26" }, generated: "2026-10-26", ...over });

test("a row passes when its report passes and has no error", () => {
  expect(passes(row("a", "device"))).toBe(true);
  expect(passes(row("a", "device", false))).toBe(false);
  expect(passes({ level: "device", name: "a", engine: "ws", error: "convert: boom" })).toBe(false);
  expect(passes({ level: "device", name: "a", engine: "ws" })).toBe(false);
});

test("retirement is ready only with every device and song passing and the listening signed off", () => {
  const ok = run([row("falcon", "device"), row("elysian", "song")]);
  expect(retirement(ok)).toEqual({ ready: true, blocking: [] });
  expect(formatRun(ok)).toContain("## Retirement: READY");
  expect(retirement(run([row("falcon", "device")], { signoff: { decision: "pending" } })).blocking).toEqual(["A/B listening sign-off pending"]);
});

test("a failing song blocks retirement and the report names it", () => {
  const bad = run([row("falcon", "device"), row("elysian", "song", false), row("engage", "song")]);
  const g = retirement(bad);
  expect(g.ready).toBe(false);
  expect(g.blocking).toEqual(["song elysian"]);
  const md = formatRun(bad);
  expect(md).toContain("## Retirement: BLOCKED");
  expect(md).toContain("- song elysian");
  expect(md).toContain("| elysian | ws |");
  expect(md).toContain("**FAIL**");
});

test("a failing device and a pipeline error block too, with the error text", () => {
  const bad = run([row("falcon", "device", false), { level: "device", name: "thunder", engine: "ws", error: "legacy render: no bank" }, row("elysian", "song")]);
  expect(retirement(bad).blocking).toEqual(["device falcon", "device thunder (legacy render: no bank)"]);
  expect(formatRun(bad)).toContain("| thunder | ws | | | | | | FAIL: legacy render: no bank |");
});

test("songs that did not run block retirement and the report says why", () => {
  const skipped = run([row("falcon", "device")], { skipped: "the synth lab songs are absent" });
  expect(retirement(skipped).blocking).toEqual(["reference songs not run: the synth lab songs are absent"]);
  const md = formatRun(skipped);
  expect(md).toContain("Skipped: the synth lab songs are absent");
  expect(md).toContain("## Retirement: BLOCKED");
});

test("Songs absent: no synth lab directory gives a skip message and no songs (the tool then exits 0)", () => {
  const s = referenceSongs({});
  expect(s.songs).toEqual([]);
  expect(s.skipped).toMatch(/absent/);
  expect(s.skipped).toContain("global-groove");
  const dir = mkdtempSync(join(tmpdir(), "songs-"));
  expect(referenceSongs({ WS_SONG_DIR: dir, TF_SONGS: dir }).songs).toEqual([]);
});

test("songs that exist are found by the manifest names, WaveSabre ones with a bank", () => {
  const dir = mkdtempSync(join(tmpdir(), "songs-"));
  writeFileSync(join(dir, "elysian.bin"), "x");
  writeFileSync(join(dir, "gm.dls"), "x");
  writeFileSync(join(dir, "no_remorse.tfm"), "x");
  const s = referenceSongs({ WS_SONG_DIR: dir, TF_SONGS: dir });
  expect(s.songs.map((x) => `${x.engine}:${x.name}`).sort()).toEqual(["tf:no-remorse", "ws:elysian"]);
  expect(s.songs.find((x) => x.name === "elysian")!.bank).toBe(join(dir, "gm.dls"));
  expect(s.songs.find((x) => x.name === "no-remorse")!.bank).toBeUndefined();
  expect(s.skipped).toContain("global-groove"); // the ones that are missing are named
});

test("speed: the floor rows show ok or FAIL by the real-time factor of the unified render", () => {
  const fast = row("elysian", "song", true, { secs: 218, legacyTime: 31, ourTime: 24 });
  const slow = row("no-remorse", "song", true, { engine: "tf", secs: 205, legacyTime: 13, ourTime: 30 });
  expect(realTime(fast)).toBeCloseTo(9.08, 1);
  const md = formatRun(run([fast, slow]));
  expect(md).toMatch(/\| elysian \| 218\.0 \| 31\.000 \| 7\.0 \| 24\.000 \| 9\.1 \| ≥ 5× ok \|/);
  expect(md).toMatch(/\| no-remorse \|.*≥ 10× \*\*FAIL\*\* \|/);
});

test("seed-judged items are listed with their spread; the size table and the listening decision are carried", () => {
  const r = row("global-groove", "song", true, { ensemble: { seeds: SEEDS, loudnessSd: 0.38, bandSd: [5, 4.7, 3.9, 3, 2.8, 1.9, 1.1, 0.7, 0.9], why: "every sound is a 2 ms noise burst" } });
  const md = formatRun(run([row("falcon", "device"), r], { sizes: "| build | raw |\n|---|---|\n| all | 1 |", signoff: { decision: "findings", by: "robson", date: "2026-10-27", notes: "Hats too bright." } }));
  expect(md).toContain("## Judged on several random seeds");
  expect(md).toContain("loudness sd 0.38 dB");
  expect(md).toContain("pass (seed mean)");
  expect(md).toContain("## Size");
  expect(md).toContain("Decision: **findings** by robson on 2026-10-27.");
  expect(md).toContain("Hats too bright.");
});

test("the report never holds audio: only text lines", () => {
  const md = formatRun(run([row("falcon", "device")]));
  expect(md.split("\n").every((l) => l.length < 800)).toBe(true);
});

test("the songs with an accepted difference are listed with their wider tolerance", () => {
  expect(ACCEPTED["global-groove"]).toMatchObject({ loudnessDb: 2.5, bandDb: 4 });
  const md = formatRun(run([row("global-groove", "song"), row("elysian", "song")]));
  expect(md).toContain("## Accepted differences");
  expect(md).toContain("- `global-groove`: loudness ±2.5 dB, bands ±4 dB");
  expect(md).not.toContain("`elysian`: loudness ±");
  expect(formatRun(run([row("elysian", "song")]))).not.toContain("Accepted differences");
});
