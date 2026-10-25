// Tunefish reference parity (opt-in, design D8.3): renders each song with our player
// (`cargo run --release -p tfsynth --example render`) and with the upstream reference built by build-ref.sh, then
// compares with the shared metrics: integrated loudness ±1 dB, octave LTAS 63 Hz–16 kHz ±3 dB, onsets within one
// 512-frame block. The reference seeds from system entropy, so renders are compared perceptually, not by samples.
//   bun tools/tf-parity/parity.ts
//   TF_REF   upstream checkout with build/render-ref (default /tmp/tf-ref); absent → skip
//   TF_SONGS colon-separated .tfm paths (default: the upstream examples in $TF_REF/media/tf4modules)
// Exit 0 pass or skipped, 1 on a failing song.
import { execFileSync } from "node:child_process";
import { existsSync, mkdirSync, readdirSync } from "node:fs";
import { basename, join, resolve } from "node:path";
import { compare, DEFAULT_TOL, formatReport, readWav, type Report } from "../synth-parity/metrics.ts";

const ROOT = resolve(import.meta.dir, "../..");
export const TOL = { ...DEFAULT_TOL, onsetFrames: 512 };

export function refDir(): string {
  return process.env.TF_REF ?? "/tmp/tf-ref";
}

export function songs(ref: string): string[] {
  if (process.env.TF_SONGS) return process.env.TF_SONGS.split(":").filter(Boolean);
  const dir = join(ref, "media/tf4modules");
  return existsSync(dir) ? readdirSync(dir).filter((f) => f.endsWith(".tfm")).sort().map((f) => join(dir, f)) : [];
}

export function run(): { name: string; report: Report }[] | null {
  const ref = refDir();
  const exe = join(ref, "build/render-ref");
  const list = songs(ref);
  if (!existsSync(exe) || list.length === 0) {
    console.log(`tf-parity: skipped (${!existsSync(exe) ? `no reference renderer at ${exe}; run tools/tf-parity/build-ref.sh` : "no songs (TF_SONGS)"})`);
    return null;
  }
  const out = join(ROOT, "build/tf-parity");
  mkdirSync(out, { recursive: true });
  execFileSync("cargo", ["build", "-q", "--release", "-p", "tfsynth", "--example", "render"], { cwd: ROOT, stdio: "inherit" });
  const ours = join(ROOT, "target/release/examples/render");
  return list.map((song) => {
    const name = basename(song, ".tfm");
    const [a, b] = [join(out, `${name}_rust.wav`), join(out, `${name}_ref.wav`)];
    execFileSync(ours, [song, a], { stdio: ["ignore", "ignore", "inherit"] });
    execFileSync(exe, [song, b]);
    return { name, report: compare(readWav(a), readWav(b), TOL) };
  });
}

if (import.meta.main) {
  const rows = run();
  if (rows) {
    rows.forEach(({ name, report }) => console.log(formatReport(name, report)));
    process.exit(rows.every((r) => r.report.pass) ? 0 : 1);
  }
}
