// Drift of a render against a stored earlier render of the same build item (change unify-v2-algorithms, design D7.3):
// loudness Δ and the worst octave-band Δ (bands more than BAND_FLOOR_DB below the loudest baseline band are skipped).
// Not a gate: items past FLAG are listed for a listen. `--baseline <dir>` stores renders, `--drift <dir>` compares.
import { copyFileSync, existsSync, mkdirSync } from "node:fs";
import { join } from "node:path";
import { loudness, octaveLtas, readWav, type Audio } from "../synth-parity/metrics.ts";
import { BAND_FLOOR_DB } from "./parity.ts";

/** Listen above these (design D7.3). */
export const FLAG = { loudnessDb: 0.5, bandDb: 1.5 };

export interface Drift {
  loudness: number;
  band: { fc: number; diff: number };
  flagged: boolean;
}

export function drift(ours: Audio, base: Audio): Drift {
  const n = Math.min(ours.ch[0].length, base.ch[0].length);
  const cut = (a: Audio): Audio => ({ rate: a.rate, ch: a.ch.map((c) => c.subarray(0, n)) });
  const [a, b] = [cut(ours), cut(base)];
  const la = loudness(a), lb = loudness(b);
  const ba = octaveLtas(a), bb = octaveLtas(b);
  const top = Math.max(...bb.map((x) => x.db));
  let band = { fc: 0, diff: 0 };
  bb.forEach((x, i) => {
    const d = ba[i].db - x.db;
    if (x.db >= top - BAND_FLOOR_DB && Math.abs(d) > Math.abs(band.diff)) band = { fc: x.fc, diff: d };
  });
  // two silent renders: no drift
  const l = Number.isFinite(la) && Number.isFinite(lb) ? la - lb : 0;
  return { loudness: l, band, flagged: Math.abs(l) > FLAG.loudnessDb || Math.abs(band.diff) > FLAG.bandDb };
}

/** Stores `wav` as `<dir>/<name>.wav`. */
export function keep(dir: string, name: string, wav: string): void {
  mkdirSync(dir, { recursive: true });
  copyFileSync(wav, join(dir, `${name}.wav`));
}

/** Drift of `wav` against `<dir>/<name>.wav`, or undefined when no baseline is stored. */
export function against(dir: string, name: string, wav: string): Drift | undefined {
  const b = join(dir, `${name}.wav`);
  return existsSync(b) ? drift(readWav(wav), readWav(b)) : undefined;
}

export const formatDrift = (name: string, d: Drift | undefined) =>
  d ? `drift ${name}: loudness ${d.loudness.toFixed(2)} dB, worst band ${d.band.diff.toFixed(2)} dB @ ${d.band.fc}${d.flagged ? "  LISTEN" : ""}` : `drift ${name}: no baseline`;
