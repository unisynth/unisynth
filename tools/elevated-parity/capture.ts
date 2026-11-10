// Capture check for songs by Mentor's 4k synth (requirement "Elevated capture check" of add-elevated-import): the
// Tunefish 3 capture comparison (alignment per 30 s window, loudness-normalised octave bands up to 8 kHz within 4 dB)
// with a consistency timing rule. These songs open on pads and carry onset-poor breaks, whose windows correlate weakly
// at random lags, so instead of "≥ 90 % of windows correlate ≥ 0.5": every window that correlates ≥ 0.5 lies within
// 2 blocks (768 frames) of the confident windows' median lag, and at least 60 % of the windows reach 0.5 (user
// decision 2026-11-10; both reference renders reach 71 %).
//   bun tools/elevated-parity/capture.ts <ours.wav> <capture.wav | slug>
//   ELEVATED_CAPTURES  capture directory for a slug (default ~/Downloads/64k-tools/elevated-captures); absent → skip
import { existsSync } from "node:fs";
import { homedir } from "node:os";
import { join } from "node:path";
import { readWav } from "../synth-parity/metrics.ts";
import { CAPTURE_TOL, type CaptureReport, compareCapture, formatCapture } from "../tf3-parity/capture.ts";

export const TIMING = { corr: CAPTURE_TOL.onsetCorr, lagFrames: CAPTURE_TOL.onsetFrames, share: 0.6 };

export const capturesDir = (): string =>
  process.env.ELEVATED_CAPTURES ?? join(homedir(), "Downloads/64k-tools/elevated-captures");

export interface Timing {
  confident: number; // windows with corr ≥ 0.5
  windows: number;
  lag: number; // median lag of the confident windows (frames)
  outliers: number; // confident windows further than 768 frames from it
  pass: boolean;
}

/** The consistency rule on a capture report's windows. */
export function timing(windows: CaptureReport["windows"]): Timing {
  const conf = windows.filter((w) => w.corr >= TIMING.corr);
  const lags = conf.map((w) => w.lag).sort((a, b) => a - b);
  const lag = lags.length ? lags[lags.length >> 1] : 0;
  const outliers = conf.filter((w) => Math.abs(w.lag - lag) > TIMING.lagFrames).length;
  const pass = windows.length > 0 && outliers === 0 && conf.length >= TIMING.share * windows.length;
  return { confident: conf.length, windows: windows.length, lag, outliers, pass };
}

export function checkCapture(ours: string, cap: string): { report: CaptureReport; timing: Timing; pass: boolean; text: string } {
  const report = compareCapture(readWav(ours), readWav(cap));
  const t = timing(report.windows);
  const pass = report.bands.every((b) => b.pass) && t.pass;
  const head = formatCapture(cap, report).split("\n").slice(1, 4);
  const text = [
    `${ours} vs capture: ${pass ? "PASS" : "FAIL"}`,
    ...head,
    `  timing: ${t.confident}/${t.windows} windows correlate ≥ ${TIMING.corr}, lag ${t.lag} frames, ${t.outliers} off it ${t.pass ? "ok" : "FAIL"}`,
  ].join("\n");
  return { report, timing: t, pass, text };
}

if (import.meta.main) {
  const [ours, capArg] = process.argv.slice(2);
  if (!ours || !capArg) {
    console.error("usage: capture.ts <ours.wav> <capture.wav | slug>");
    process.exit(2);
  }
  const cap = capArg.endsWith(".wav") ? capArg : join(capturesDir(), `${capArg}.wav`);
  if (!existsSync(cap)) {
    console.log(`elevated capture: skipped (no capture ${cap}; run tools/elevated-parity/captures.sh)`);
    process.exit(0);
  }
  const r = checkCapture(ours, cap);
  console.log(r.text);
  process.exit(r.pass ? 0 : 1);
}
