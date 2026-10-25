import { expect, test } from "bun:test";
import { mkdtempSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { compare, DEFAULT_TOL, loudness, octaveLtas, onsets, readWav, writeWav, type Audio } from "./metrics.ts";

const RATE = 44100;

function audio(n: number, f: (i: number, ch: number) => number): Audio {
  const ch = [new Float32Array(n), new Float32Array(n)];
  for (let c = 0; c < 2; c++) for (let i = 0; i < n; i++) ch[c][i] = f(i, c);
  return { rate: RATE, ch };
}

// deterministic "music": decaying noise bursts every 0.25 s plus a 220 Hz tone
function music(n = RATE * 4, gain = 1, shift = 0): Audio {
  let s = 1;
  const rnd = () => ((s = (s * 1103515245 + 12345) >>> 0) / 2 ** 32) * 2 - 1;
  const noise = Float32Array.from({ length: n }, rnd);
  return audio(n, (i, c) => {
    const j = i - shift;
    const t = ((j % 11025) + 11025) % 11025;
    const burst = j >= 0 ? noise[i] * Math.exp(-t / 800) : 0;
    return gain * (0.3 * burst + 0.1 * Math.sin((2 * Math.PI * 220 * i) / RATE + c));
  });
}

test("BS.1770: full-scale 997 Hz sine in one channel reads -3.01 LUFS", () => {
  const a = audio(RATE * 3, (i, c) => (c === 0 ? Math.sin((2 * Math.PI * 997 * i) / RATE) : 0));
  expect(loudness(a)).toBeCloseTo(-3.01, 1);
});

test("loudness gating ignores silence (only the three edge blocks count)", () => {
  const loud = audio(RATE * 2, (i) => 0.1 * Math.sin((2 * Math.PI * 997 * i) / RATE));
  const withGap = audio(RATE * 6, (i) => (i < RATE * 2 ? loud.ch[0][i] : 0));
  // ungated, 4 s of silence would pull it down by 10·log10(3) = 4.8 dB
  expect(Math.abs(loudness(withGap) - loudness(loud))).toBeLessThan(0.5);
});

test("octave LTAS puts a 1 kHz sine in the 1 kHz band", () => {
  const a = audio(RATE * 2, (i) => Math.sin((2 * Math.PI * 1000 * i) / RATE));
  const b = octaveLtas(a);
  expect(b.map((x) => x.fc)).toEqual([63, 125, 250, 500, 1000, 2000, 4000, 8000, 16000]);
  const k = b.findIndex((x) => x.fc === 1000);
  for (const [i, x] of b.entries()) if (i !== k) expect(x.db).toBeLessThan(b[k].db - 30);
});

test("onsets are found at the bursts", () => {
  const o = onsets(music());
  expect(o.length).toBe(16);
  // constant detector bias cancels in comparisons; the spacing is what must be right
  o.slice(1).forEach((f, i) => expect(Math.abs(f - o[1] - i * 11025)).toBeLessThanOrEqual(64));
});

test("identical renders pass", () => {
  const r = compare(music(), music());
  expect(r.pass).toBe(true);
  expect(r.loudness.diff).toBe(0);
  expect(r.onsets.matched).toBe(r.onsets.total);
});

test("+3 dB fails on loudness and every band", () => {
  const r = compare(music(RATE * 4, Math.SQRT2), music());
  expect(r.pass).toBe(false);
  expect(r.loudness.diff).toBeCloseTo(3.01, 1);
  expect(r.loudness.pass).toBe(false);
  expect(r.bands.every((b) => Math.abs(b.diff - 3.01) < 0.05 || b.ref < -100)).toBe(true);
});

test("+0.5 dB passes; a spectral tilt fails only the affected bands", () => {
  expect(compare(music(RATE * 4, 10 ** (0.5 / 20)), music()).pass).toBe(true);
  const ref = music();
  // one-pole lowpass at ~1 kHz: highs drop well over 3 dB
  const lp = audio(ref.ch[0].length, () => 0);
  for (let c = 0; c < 2; c++) {
    let z = 0;
    const k = 1 - Math.exp((-2 * Math.PI * 1000) / RATE);
    for (let i = 0; i < lp.ch[c].length; i++) lp.ch[c][i] = z += k * (ref.ch[c][i] - z);
  }
  const r = compare(lp, ref);
  expect(r.bands.find((b) => b.fc === 125)!.pass).toBe(true);
  expect(r.bands.find((b) => b.fc === 8000)!.pass).toBe(false);
  expect(r.pass).toBe(false);
});

test("onsets within one 100-frame step pass; 300 frames late fails", () => {
  expect(compare(music(RATE * 4, 1, 60), music()).onsets.pass).toBe(true);
  const late = compare(music(RATE * 4, 1, 300), music());
  expect(late.onsets.pass).toBe(false);
  expect(late.pass).toBe(false);
  expect(compare(music(RATE * 4, 1, 300), music(), { ...DEFAULT_TOL, onsetFrames: 512 }).onsets.pass).toBe(true);
});

test("WAV round trip (16-bit)", () => {
  const p = join(mkdtempSync(join(tmpdir(), "64k-sp-")), "a.wav");
  const a = music(1000);
  writeWav(p, a);
  const b = readWav(p);
  expect(b.rate).toBe(RATE);
  expect(b.ch[1].length).toBe(1000);
  for (let i = 0; i < 1000; i++) expect(Math.abs(b.ch[1][i] - a.ch[1][i])).toBeLessThan(1 / 32000);
});
