import { expect, test } from "bun:test";
import { join } from "node:path";
import type { Audio, Report } from "../synth-parity/metrics.ts";
import { clickTrack, dropLead, formatRun, judgePhrase, judgeSong, leadFrames, onsetMatch, parseTriggers, passes, phraseRef, pingpongSeconds, type Row } from "./cauldron.ts";

/** A decaying 220 Hz + 9 kHz tone burst every 0.25 s: has onsets and energy in the high bands. */
function tones(seconds = 2, hi = 0.3): Audio {
  const rate = 44100, n = rate * seconds, x = new Float32Array(n);
  for (let i = 0; i < n; i++) {
    const t = (i % (rate / 4)) / rate, env = Math.exp(-t * 12);
    x[i] = env * (0.5 * Math.sin((2 * Math.PI * 220 * i) / rate) + hi * Math.sin((2 * Math.PI * 9000 * i) / rate));
  }
  return { rate, ch: [x, x.slice()] };
}

test("a reference compared with itself has zero differences and passes", () => {
  const a = tones();
  const r = judgeSong(a, a);
  expect(r.pass).toBe(true);
  expect(Math.abs(r.loudness.diff)).toBe(0);
  for (const b of r.bands) if (Number.isFinite(b.diff)) expect(Math.abs(b.diff)).toBe(0);
}, 30000);

test("phrases judge only octave bands lying wholly below their stored Nyquist", () => {
  // the 8 kHz band spans 5.7–11.3 kHz: past a Nyquist inside it, its level is the playback filters', not the synth's
  const ref = tones(1, 0.3), ours = tones(1, 0.0); // ours lacks the 9 kHz partial
  expect(judgeSong(ours, ref).pass).toBe(false);
  expect(judgePhrase(ours, ref, 4430).bands.find((b) => b.fc === 8000)!.pass).toBe(true);
  expect(judgePhrase(ours, ref, 8860).bands.find((b) => b.fc === 8000)!.pass).toBe(true);
  expect(judgePhrase(ours, ref, 11400).bands.find((b) => b.fc === 8000)!.pass).toBe(false);
}, 30000);

test("a mono phrase reference is compared with our mid channel", () => {
  // the phrase references are the stored mono samples; a centred render has the same signal in both channels
  const stereo = tones(1, 0.3), mono: Audio = { rate: stereo.rate, ch: [stereo.ch[0]] };
  const r = judgePhrase(stereo, mono, 20000);
  expect(Math.abs(r.loudness.diff)).toBeLessThan(1e-9);
  expect(r.pass).toBe(true);
}, 30000);

test("the converter's ping-pong seconds are parsed, 0 when absent", () => {
  expect(pingpongSeconds("warning: Jace14 ping-pong\npingpong-seconds 12.5\n")).toBe(12.5);
  expect(pingpongSeconds("")).toBe(0);
});

test("a phrase's lead-in is parsed and dropped", () => {
  expect(leadFrames("lead-frames 731\n")).toBe(731);
  expect(leadFrames("")).toBe(0);
  const a: Audio = { rate: 44100, ch: [new Float32Array([0, 0, 1, 2]), new Float32Array([0, 0, 3, 4])] };
  expect(Array.from(dropLead(a, 2).ch[1])).toEqual([3, 4]);
});

const report = (pass: boolean): Report => ({
  loudness: { ours: -20, ref: -20.1, diff: 0.1, pass },
  bands: [63, 125, 250, 500, 1000, 2000, 4000, 8000, 16000].map((fc) => ({ fc, ours: 10, ref: 10, diff: fc === 500 ? -0.4 : 0.2, pass: true })),
  onsets: { total: 4, matched: 4, extra: 0, maxOffset: 0, pass: true, lag: 0, corr: 0.97 },
  pass,
});

test("the report has a song section with ping-pong seconds and a phrase section", () => {
  const rows: Row[] = [
    { name: "jizz", kind: "song", report: report(true), pingpong: 0 },
    { name: "stash_song1", kind: "song", report: report(false), pingpong: 419.6, warnings: ["warning: x"] },
    { name: "jizz_Probe11", kind: "phrase", error: "convert: boom" },
  ];
  expect(passes(rows[0])).toBe(true);
  const md = formatRun(rows, "2026-10-30", { decision: "pending" });
  expect(md).toContain("| jizz | PASS | 0.10 | -0.40 @ 500 | 0.97 | 0.00 |");
  expect(md).toContain("| stash_song1 | FAIL | 0.10 | -0.40 @ 500 | 0.97 | 419.60 | 1 converter warning(s) |");
  expect(md).toContain("| jizz_Probe11 | ERROR |");
  expect(md).toContain("Pending:");
});

test("the CLI skips with exit 0 when the reference material is absent", () => {
  const p = Bun.spawnSync(["bun", join(import.meta.dir, "cauldron.ts")], { env: { ...process.env, CAULDRON_OUT: "/nonexistent/cauldron" }, stdout: "pipe" });
  expect(p.exitCode).toBe(0);
  expect(p.stdout.toString()).toContain("skip:");
});

test("decimated phrases are judged against the alias-free reference", () => {
  expect(phraseRef({ name: "x", decim: 2 })).toMatch(/listen\/ref44bw\/x\.wav$/);
  expect(phraseRef({ name: "x", decim: 4 })).toMatch(/listen\/ref44bw\/x\.wav$/);
  expect(phraseRef({ name: "x", decim: 1 })).toMatch(/listen\/ref44\/x\.wav$/);
});

test("trigger lists become click tracks whose onsets match the triggers", () => {
  const t = parseTriggers("0.000000 1.0000\n0.500000 0.0000\n1.000000 0.5000\n1.250000 0.8000\n");
  expect(t).toEqual([{ at: 0, vol: 1, offset: 0 }, { at: 0.5, vol: 0, offset: 0 }, { at: 1, vol: 0.5, offset: 0 }, { at: 1.25, vol: 0.8, offset: 0 }]);
  expect(parseTriggers("0.5 1 1024")[0].offset).toBe(1024);
  const clicks = clickTrack(t, 2, 44100);
  expect(clicks.ch[0].length).toBe(88200);
  expect(clicks.ch[0][22050 + 10]).toBe(0); // a silent trigger is no click
  // the same clicks 40 frames later: every onset matched, lag within a flux hop of 40
  const later = clickTrack(t.map((x) => ({ ...x, at: x.at + 40 / 44100 })), 2, 44100);
  const m = onsetMatch(later, clicks, 100);
  expect(m.matched).toBe(1);
  expect(m.offset).toBe(40);
  // 300 frames off is outside 100
  const off = clickTrack(t.map((x) => ({ ...x, at: x.at + 300 / 44100 })), 2, 44100);
  expect(onsetMatch(off, clicks, 100).matched).toBe(0);
}, 30000);
