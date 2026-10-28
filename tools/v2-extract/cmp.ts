// Quick V2 parity check with the song settings of tools/unisynth-parity (onset-strength alignment, 24 dB band floor),
// both renders cut to the shorter one. bun tools/v2-extract/cmp.ts <ours.wav> <ref.wav>
import { compare, DEFAULT_TOL, formatReport, readWav } from "../synth-parity/metrics.ts";
import { BAND_FLOOR_DB, MIN_ALIGNMENT } from "../unisynth-parity/parity.ts";

const [a, b] = process.argv.slice(2);
const ours = readWav(a), ref = readWav(b);
const n = Math.min(ours.ch[0].length, ref.ch[0].length);
const cut = (x: typeof ours) => ({ rate: x.rate, ch: x.ch.map((c) => c.subarray(0, n)) });
const r = compare(cut(ours), cut(ref), { ...DEFAULT_TOL, bandFloorDb: BAND_FLOOR_DB, onsetAlignment: MIN_ALIGNMENT });
console.log(formatReport(a, r));
process.exit(r.pass ? 0 : 1);
