// bun tools/synth-parity/cli.ts <ours.wav> <ref.wav> [--onset-frames N] [--json]
// Prints the parity report; exit 1 on FAIL.
import { compare, DEFAULT_TOL, formatReport, readWav } from "./metrics.ts";

const args = process.argv.slice(2);
const flag = (k: string) => {
  const i = args.indexOf(k);
  return i < 0 ? undefined : args.splice(i, 2)[1];
};
const onsetFrames = Number(flag("--onset-frames") ?? DEFAULT_TOL.onsetFrames);
const json = args.includes("--json") && args.splice(args.indexOf("--json"), 1).length > 0;
if (args.length !== 2) {
  console.error("usage: cli.ts <ours.wav> <ref.wav> [--onset-frames N] [--json]");
  process.exit(2);
}
const r = compare(readWav(args[0]), readWav(args[1]), { ...DEFAULT_TOL, onsetFrames });
console.log(json ? JSON.stringify(r) : formatReport(args[0], r));
process.exit(r.pass ? 0 : 1);
