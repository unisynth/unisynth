// Coarse comparison of a render against a lossy audio capture of the released intro (design D2a, requirement
// "Tunefish 3 capture check" of add-tf3-import). The capture has an unknown start, gain and codec, so:
//   - start: both signals' first sound above −60 dBFS give a coarse offset, refined per 30 s window by the shared
//     onset cross-correlation (`alignment`, ±0.5 s); the report lists the lag of every window (drift shows up there);
//   - gain: the capture is scaled to the render's integrated loudness (the difference is reported, not judged);
//   - bands: octave bands up to 8 kHz (5.6–11.3 kHz; the 16 kHz band is above the codec's reliable range) within 4 dB;
//   - onsets: in at least 90 % of the windows the onset-strength curves correlate ≥ 0.5 at a lag within 2 blocks
//     (768 frames) of the common offset. Peak-picked onsets are not used: on a lossy capture the picker keeps a
//     different, much smaller set of hits (Transplant: 11 against 25 in the reference render).
//   bun tools/tf3-parity/capture.ts <ours.wav> <capture.wav | slug>
//   TF3_CAPTURES  capture directory for a slug (default ~/Downloads/64k-tools/tf3-captures); absent → skip, exit 0
import { existsSync } from "node:fs";
import { homedir } from "node:os";
import { join } from "node:path";
import { type Audio, FLUX_HOP, loudness, novelty, octaveLtas, onsetFlux, readWav } from "../synth-parity/metrics.ts";

export const CAPTURE_TOL = { bandDb: 4, onsetFrames: 768, onsetMatch: 0.9, onsetCorr: 0.5, maxBandHz: 8000, windowS: 30, lagS: 0.5 };

export const capturesDir = (): string => process.env.TF3_CAPTURES ?? join(homedir(), "Downloads/64k-tools/tf3-captures");

export interface CaptureReport {
  offset: number; // capture frame that lines up with frame 0 of ours (coarse + median window lag)
  gainDb: number; // ours − capture integrated loudness
  windows: { start: number; lag: number; corr: number }[]; // start in seconds of ours; lag in frames, ours vs capture
  bands: { fc: number; diff: number; pass: boolean }[];
  onsets: { windows: number; matched: number; pass: boolean }; // windows whose timing agrees
  pass: boolean;
}

const slice = (a: Audio, from: number, to: number): Audio => {
  const s = Math.max(0, Math.min(from, a.ch[0].length)), e = Math.max(s, Math.min(to, a.ch[0].length));
  return { rate: a.rate, ch: a.ch.map((c) => c.subarray(s, e)) };
};
const scale = (a: Audio, g: number): Audio => ({ rate: a.rate, ch: a.ch.map((c) => c.map((x) => x * g)) });

/** First frame whose 10 ms peak exceeds −60 dBFS. */
export function firstSound(a: Audio): number {
  const step = Math.round(a.rate / 100), thr = 10 ** (-60 / 20);
  for (let i = 0; i < a.ch[0].length; i += step)
    for (const c of a.ch) for (let j = i; j < Math.min(i + step, c.length); j++) if (Math.abs(c[j]) > thr) return i;
  return 0;
}

/**
 * The shared `alignment` rule on curve windows: lag (curve steps, + = ours later) with the highest normalised
 * correlation of `x[f + lag]·y[f]` within ±L. The curves are computed once per signal; windows only slice them.
 */
function windowAlign(x: Float64Array, y: Float64Array, L: number): { lag: number; corr: number } {
  const dot = (lag: number) => {
    let s = 0;
    for (let f = Math.max(0, -lag); f < y.length && f + lag < x.length; f++) s += x[f + lag] * y[f];
    return s;
  };
  const norm = Math.sqrt(x.reduce((p, v) => p + v * v, 0) * y.reduce((p, v) => p + v * v, 0)) || 1;
  let best = 0, bestC = -Infinity;
  for (let lag = -L; lag <= L; lag++) {
    const c = dot(lag) / norm;
    if (c > bestC) [best, bestC] = [lag, c];
  }
  return { lag: best, corr: bestC };
}

export function compareCapture(ours: Audio, cap: Audio, windowS = CAPTURE_TOL.windowS): CaptureReport {
  if (ours.rate !== cap.rate) throw new Error(`rates differ: ${ours.rate} vs ${cap.rate}`);
  const rate = ours.rate, win = Math.round((windowS * rate) / FLUX_HOP), L = Math.ceil((CAPTURE_TOL.lagS * rate) / FLUX_HOP);
  const coarse = firstSound(cap) - firstSound(ours);
  // Curves are computed once per whole signal: slicing per window would put a false onset (the flux frame reading the
  // zero padding before the cut) at every window start, and the shared spike wins the correlation at lag 0. The one
  // spike left, at each signal's start, is cleared the same way.
  const curve = (a: Audio) => { const n = novelty(onsetFlux(a), rate); n.fill(0, 0, 1024 / FLUX_HOP); return n; };
  const [x, y] = [curve(ours), curve(slice(cap, Math.max(0, coarse), Infinity))];
  const shift = Math.round(Math.min(0, coarse) / FLUX_HOP); // a capture that starts before ours (negative coarse)
  const windows: CaptureReport["windows"] = [];
  for (let s = 0; s + win / 2 < x.length; s += win) {
    const yo = s + shift;
    if (yo < 0 || y.length - yo < win / 2) break;
    const al = windowAlign(x.subarray(s, s + win), y.subarray(yo, yo + win), L);
    windows.push({ start: (s * FLUX_HOP) / rate, lag: al.lag * FLUX_HOP, corr: al.corr });
  }
  const lags = windows.filter((w) => w.corr > 0.2).map((w) => w.lag).sort((a, b) => a - b);
  const median = lags.length ? lags[lags.length >> 1] : 0;
  const offset = coarse - median; // ours lagging by +lag means the capture's material sits earlier
  const aligned = slice(cap, offset, offset + ours.ch[0].length);
  const gainDb = loudness(ours) - loudness(aligned);
  const [bo, bc] = [octaveLtas(ours), octaveLtas(scale(aligned, 10 ** (gainDb / 20)))];
  const bands = bo.filter((b) => b.fc <= CAPTURE_TOL.maxBandHz).map((b, i) => {
    const diff = b.db - bc[i].db;
    return { fc: b.fc, diff, pass: Math.abs(diff) <= CAPTURE_TOL.bandDb };
  });
  const matched = windows.filter((w) => w.corr >= CAPTURE_TOL.onsetCorr && Math.abs(w.lag - median) <= CAPTURE_TOL.onsetFrames).length;
  const onsets = { windows: windows.length, matched, pass: windows.length > 0 && matched >= CAPTURE_TOL.onsetMatch * windows.length };
  return { offset, gainDb, windows, bands, onsets, pass: bands.every((b) => b.pass) && onsets.pass };
}

export function formatCapture(name: string, r: CaptureReport, rate = 44100): string {
  const f = (x: number) => x.toFixed(2);
  return [
    `${name} vs capture: ${r.pass ? "PASS" : "FAIL"}`,
    `  offset ${(r.offset / rate).toFixed(3)} s, gain ${f(r.gainDb)} dB (normalised, not judged)`,
    `  window lag/corr: ${r.windows.map((w) => `${w.start.toFixed(0)}s:${w.lag}/${f(w.corr)}`).join(" ")}`,
    `  octaves ${r.bands.map((b) => `${b.fc}:${f(b.diff)}${b.pass ? "" : "!"}`).join(" ")}`,
    `  onset timing agrees in ${r.onsets.matched}/${r.onsets.windows} windows ${r.onsets.pass ? "ok" : "FAIL"}`,
  ].join("\n");
}

if (import.meta.main) {
  const [ours, capArg] = process.argv.slice(2);
  if (!ours || !capArg) {
    console.error("usage: capture.ts <ours.wav> <capture.wav | slug>");
    process.exit(2);
  }
  const cap = capArg.endsWith(".wav") ? capArg : join(capturesDir(), `${capArg}.wav`);
  if (!existsSync(cap)) {
    console.log(`tf3 capture: skipped (no capture ${cap}; run tools/tf3-parity/captures.sh)`);
    process.exit(0);
  }
  const r = compareCapture(readWav(ours), readWav(cap));
  console.log(formatCapture(capArg, r));
  process.exit(r.pass ? 0 : 1);
}
