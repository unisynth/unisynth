// Plain-XM parity (change add-plain-xm-import, task 5.2): the 1998 softsynth intros converted with their generator
// maps (`tools/xm-gen/maps`) against `openmpt123` renders of their reference XMs, the received XM with every generated
// sample as the map's build generated it (`tools/xm-gen/run.sh` writes `$ZENEKK_DIR/xm-gen/<intro>.reference.xm`).
// Shared metrics: 1 dB loudness, 3 dB per octave band, onset alignment. A conversion whose engine is not in this
// build yet is "pending", not a failure; the received XM against the reference is reported for reading (Theta's XM is
// a later build than its executable).
//
//   bun tools/unisynth-parity/plain-xm.ts [--report path] [intro…]
//
// Material from $ZENEKK_DIR (default ~/Downloads/zenekk); absent → skip, exit 0. Exit 1 when a conversion fails or a
// judged song does not pass. Audio stays in $ZENEKK_DIR/xm-gen/parity.
import { copyFileSync, existsSync, mkdirSync, writeFileSync } from "node:fs";
import { homedir } from "node:os";
import { join, resolve } from "node:path";
import { readWav, type Audio, type Report } from "../synth-parity/metrics.ts";
import { dropLead, judgeSong, leadFrames, PARITY_OPTIONS } from "./cauldron.ts";
import { PROFILE } from "./parity.ts";

const ROOT = resolve(import.meta.dir, "../..");
export const ZENEKK = process.env.ZENEKK_DIR ?? join(homedir(), "Downloads/zenekk");
export const MAPS = join(ROOT, "tools/xm-gen/maps");

/** Intro → its XM in $ZENEKK_DIR. */
export const INTROS: Record<string, string> = {
  "fudge-asc": "Fudge - Alien Sex Clone.xm",
  "quad-theta": "Quad - Theta.xm",
  "quad-pure": "Quad - Pure.xm",
};

export type Status = "pass" | "fail" | "pending" | "skip";

export interface Row {
  name: string;
  status: Status;
  note?: string;
  /** Ours against the reference. */
  report?: Report;
  /** The received XM against the reference (for reading). */
  received?: Report;
}

/** The engine a conversion lacks, from `synth-convert`'s error. */
export const engineMissing = (err: string) => /engine `([^`]+)` \(.*\) is not available/.exec(err)?.[1];

/** A run passes when nothing failed (pending and skipped rows do not fail it). */
export const passes = (rows: Row[]) => rows.every((r) => r.status !== "fail");

function sh(cmd: string[]): { ok: boolean; err: string } {
  const p = Bun.spawnSync(cmd, { cwd: ROOT, stdout: "pipe", stderr: "pipe" });
  return { ok: p.exitCode === 0, err: p.stderr.toString().trim() };
}

/** `openmpt123` at 44.1 kHz of `xm`, copied into `work` as `<tag>.xm` first (`--render` writes `<file>.wav` beside
 * its input). */
function render(xm: string, work: string, tag: string): Audio {
  const copy = join(work, `${tag}.xm`);
  copyFileSync(xm, copy);
  const r = sh(["openmpt123", "--quiet", "--render", "--force", "--samplerate", "44100", "--output-type", "wav", copy]);
  if (!r.ok) throw new Error(`openmpt123: ${r.err.slice(-200)}`);
  return readWav(`${copy}.wav`);
}

export function runIntro(name: string, work: string): Row {
  const xm = join(ZENEKK, INTROS[name]), ref = join(ZENEKK, "xm-gen", `${name}.reference.xm`), map = join(MAPS, `${name}.json`);
  for (const f of [xm, ref, map]) if (!existsSync(f)) return { name, status: "skip", note: `${f} absent (tools/xm-gen/run.sh)` };
  try {
    const refAudio = render(ref, work, `${name}.reference`);
    const received = judgeSong(render(xm, work, `${name}.received`), refAudio);
    const conv = join(work, `${name}.song`), wav = join(work, `${name}.unisynth.wav`);
    const c = sh([join(ROOT, "target", PROFILE, "synth-convert"), "xm", xm, map, conv, ...PARITY_OPTIONS]);
    if (!c.ok) {
      const engine = engineMissing(c.err);
      return engine ? { name, status: "pending", note: `engine \`${engine}\` not in this build yet`, received } : { name, status: "fail", note: `convert: ${c.err.split("\n").pop()}`, received };
    }
    const u = sh([join(ROOT, "target", PROFILE, "examples", "render"), conv, wav]);
    if (!u.ok) return { name, status: "fail", note: `render: ${u.err.slice(-200)}`, received };
    const report = judgeSong(dropLead(readWav(wav), leadFrames(c.err)), refAudio);
    return { name, status: report.pass ? "pass" : "fail", report, received };
  } catch (e) {
    return { name, status: "fail", note: String((e as Error).message) };
  }
}

const fmt = (r?: Report) =>
  r ? `${r.loudness.diff.toFixed(2)} dB, bands ≤ ${Math.max(...r.bands.map((b) => Math.abs(b.diff))).toFixed(2)} dB, ${r.pass ? "within" : "outside"} tolerance` : "–";

export function formatRun(rows: Row[], generated: string): string {
  const L = [
    "# Plain-XM parity (1998 softsynth intros)",
    "",
    `Generated ${generated} by \`bun tools/unisynth-parity/plain-xm.ts\`. Reference: \`openmpt123\` (44.1 kHz) of each intro's XM with every generated sample as the build its generator map was extracted from generated it (\`tools/xm-gen\`); ours: \`synth-convert xm <xm> <map> ${PARITY_OPTIONS.join(" ")}\` → \`unisynth\` (\`--profile ${PROFILE}\`). Loudness within 1 dB, octave bands within 3 dB, onset alignment. "Received XM" compares the XM as received with the reference, for reading. Audio stays in \`$ZENEKK_DIR/xm-gen/parity\`.`,
    "",
    "| Intro | Status | Ours vs reference | Received XM vs reference | Note |",
    "|---|---|---|---|---|",
    ...rows.map((r) => `| ${r.name} | ${r.status} | ${fmt(r.report)} | ${fmt(r.received)} | ${r.note ?? ""} |`),
  ];
  return L.join("\n") + "\n";
}

function main() {
  const a = process.argv.slice(2);
  const opt = (k: string) => (a.includes(k) ? a.splice(a.indexOf(k), 2)[1] : undefined);
  const reportPath = opt("--report") ?? join(import.meta.dir, "plain-xm-report.md");
  if (!Object.values(INTROS).some((x) => existsSync(join(ZENEKK, x)))) {
    console.log(`skip: plain-XM material absent (${ZENEKK})`);
    process.exit(0);
  }
  const b = sh(["cargo", "build", "-q", "--profile", PROFILE, "-p", "synth-convert", "--bin", "synth-convert"]);
  const e = sh(["cargo", "build", "-q", "--profile", PROFILE, "-p", "unisynth", "--features", "std", "--example", "render"]);
  if (!b.ok || !e.ok) throw new Error(`cargo build failed:\n${b.err}\n${e.err}`);
  const work = join(ZENEKK, "xm-gen", "parity");
  mkdirSync(work, { recursive: true });
  const rows = Object.keys(INTROS)
    .filter((n) => !a.length || a.includes(n))
    .map((n) => runIntro(n, work));
  for (const r of rows) console.log(`${r.name}: ${r.status} ${r.note ?? ""}  (received XM vs reference: ${fmt(r.received)})`);
  if (!a.length) writeFileSync(reportPath, formatRun(rows, new Date().toISOString().slice(0, 10)));
  process.exit(passes(rows) ? 0 : 1);
}

if (import.meta.main) main();
