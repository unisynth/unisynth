import { describe, expect, test } from "bun:test";
import { spawnSync } from "node:child_process";
import { join } from "node:path";
import type { Audio } from "../synth-parity/metrics.ts";
import { compareCapture, firstSound } from "./capture.ts";

/** 12 s of decaying noise bursts on a 0.5 s grid, after `lead` frames of silence. */
function bursts(lead: number, gain = 1, seed = 7): Audio {
  const rate = 44100, n = lead + 12 * rate;
  const l = new Float32Array(n), r = new Float32Array(n);
  let s = seed;
  const rnd = () => ((s = (s * 16807) % 2147483647) / 2147483647) * 2 - 1;
  for (let t = 0; t < 12 * rate; t++) {
    const env = Math.exp(-(t % 22050) / 2000) * gain * 0.3;
    l[lead + t] = rnd() * env;
    r[lead + t] = rnd() * env;
  }
  return { rate, ch: [l, r] };
}

describe("tf3 capture check", () => {
  test("first sound", () => {
    expect(firstSound(bursts(44100))).toBe(44100);
  });

  test("a delayed, quieter copy aligns and passes", () => {
    const r = compareCapture(bursts(0), bursts(3 * 44100, 0.5), 4);
    expect(Math.abs(r.offset - 3 * 44100)).toBeLessThanOrEqual(64);
    expect(r.gainDb).toBeCloseTo(6.02, 1);
    expect(r.windows.length).toBe(3);
    expect(r.pass).toBe(true);
  }, 30000);

  test("a capture of different material fails timing", () => {
    const tone: Audio = { rate: 44100, ch: bursts(0).ch.map((c) => c.map((_, i) => Math.sin(i * 0.05) * 0.1)) };
    const r = compareCapture(bursts(0), tone, 4);
    expect(r.onsets.pass).toBe(false);
  }, 30000);

  test("skips without the capture", () => {
    const p = spawnSync("bun", [join(import.meta.dir, "capture.ts"), "x.wav", "nothing"], { env: { ...process.env, TF3_CAPTURES: "/nonexistent" }, encoding: "utf8" });
    expect(p.status).toBe(0);
    expect(p.stdout).toContain("skipped");
  });

  test("captures.sh skips without its tools", () => {
    const p = spawnSync("/bin/bash", [join(import.meta.dir, "captures.sh")], { env: { PATH: "/nonexistent", TF3_CAPTURES: "/tmp/x" }, encoding: "utf8" });
    expect(p.status).toBe(0);
    expect(p.stdout).toContain("skipped");
  });
});
