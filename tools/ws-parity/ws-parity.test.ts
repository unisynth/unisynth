import { expect, test } from "bun:test";
import { spawnSync } from "node:child_process";
import { join } from "node:path";
import { MIN_SNR_DB, snr } from "./ws-parity.ts";
import type { Audio } from "../synth-parity/metrics.ts";

const tone = (n: number, g = 1): Audio => ({
  rate: 44100,
  ch: [0, 1].map(() => Float32Array.from({ length: n }, (_, i) => g * Math.sin(i / 7))),
});

test("snr: identical is infinite, a 0.1 % error is 60 dB", () => {
  expect(snr(tone(1000), tone(1000))).toBe(Infinity);
  expect(snr(tone(1000, 1.001), tone(1000))).toBeCloseTo(60, 1);
  expect(snr(tone(1000, 1.002), tone(1000))).toBeLessThan(MIN_SNR_DB);
});

test("skips cleanly without the upstream checkout", () => {
  const r = spawnSync("bun", [join(import.meta.dir, "ws-parity.ts")], {
    env: { ...process.env, WS_LAB: "/nonexistent/WaveSabre" },
    encoding: "utf8",
  });
  expect(r.status).toBe(0);
  expect(r.stdout).toContain("skipped");
});

// Full run (builds upstream, ~1 min): WS_PARITY=1 bun test tools/ws-parity
test.if(!!process.env.WS_PARITY)("every device ≥ 60 dB against upstream", () => {
  const r = spawnSync("bun", [join(import.meta.dir, "ws-parity.ts")], { encoding: "utf8", stdio: ["ignore", "pipe", "inherit"] });
  console.log(r.stdout);
  expect(r.status).toBe(0);
}, 600_000);
