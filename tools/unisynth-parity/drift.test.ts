import { expect, test } from "bun:test";
import type { Audio } from "../synth-parity/metrics.ts";
import { drift, driftOf, formatDrift, profile } from "./drift.ts";

const RATE = 44100;
/** Two seconds of a stereo sine at `hz`, amplitude `amp`, plus a quieter `hz2` partial. */
function tone(amp: number, hz = 440, hz2 = 3000, amp2 = 0.1): Audio {
  const n = 2 * RATE;
  const x = Float32Array.from({ length: n }, (_, i) => amp * Math.sin((2 * Math.PI * hz * i) / RATE) + amp2 * Math.sin((2 * Math.PI * hz2 * i) / RATE));
  return { rate: RATE, ch: [x, x.slice()] };
}

test("a render against itself has no drift", () => {
  const d = drift(tone(0.5), tone(0.5));
  expect(d.loudness).toBe(0);
  expect(d.band.diff).toBe(0);
  expect(d.flagged).toBe(false);
});

test("a 1 dB quieter render drifts by about −1 dB and is flagged", () => {
  const g = 10 ** (-1 / 20);
  const d = drift(tone(0.5 * g, 440, 3000, 0.1 * g), tone(0.5));
  expect(d.loudness).toBeCloseTo(-1, 1);
  expect(d.flagged).toBe(true);
});

test("a change in one band only shows as the worst band", () => {
  const d = drift(tone(0.5, 440, 3000, 0.05), tone(0.5));
  expect(d.band.fc).toBe(4000);
  expect(d.band.diff).toBeCloseTo(-6, 0);
  expect(formatDrift("x", d)).toContain("LISTEN");
});

test("renders of different length compare over the shorter", () => {
  const a = tone(0.5), b = tone(0.5);
  const short: Audio = { rate: RATE, ch: a.ch.map((c) => c.subarray(0, RATE)) };
  expect(drift(short, b).loudness).toBeCloseTo(0, 6);
});

test("drift from stored profiles equals drift from the renders", () => {
  const a = tone(0.5, 440, 3000, 0.05), b = tone(0.5);
  const d = driftOf(profile(a), profile(b));
  expect(d).toEqual(drift(a, b));
  expect(profile(a).frames).toBe(2 * RATE);
});
