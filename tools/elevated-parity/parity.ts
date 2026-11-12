// Parity of songs by Mentor's 4k synth (requirement "Elevated reference parity" of add-elevated-import): each song's
// music.asm is converted by `synth-convert elevated`, rendered by `unisynth`, and compared with the original code's
// render (build-ref.sh) with the song-parity tolerances of tools/unisynth-parity (1 dB loudness, 3 dB per octave band
// above a 24 dB floor, timing by onset-strength alignment: correlation ≥ 0.5 within the onset window; peak-picked onsets
// are listed, not judged, as there), both renders cut to the shorter one. With --solo, every synth machine is also compared alone (converter
// --solo k against the reference's SOLO=1 renders). With --capture, the converted song is also checked against the
// YouTube capture (capture.ts). Writes report.md (metrics, never audio) next to this file.
//
//   bun tools/elevated-parity/parity.ts [--solo] [--capture] [--report path] [song…]     songs: atrium, elevated, receptor
//   ELEVATED_SRC / ELEVATED_REF / RECEPTOR_SRC as in build-ref.sh; run build-ref.sh (SOLO=1 for --solo) first
// Exit 0 when every song that ran passed, or when the sources or reference renders are absent (skipped); 1 on a failure.
import { existsSync, mkdirSync, writeFileSync } from "node:fs";
import { homedir } from "node:os";
import { join, resolve } from "node:path";
import { type Audio, compare, DEFAULT_TOL, readWav, type Report, type Tolerance } from "../synth-parity/metrics.ts";
import { BAND_FLOOR_DB, MIN_ALIGNMENT } from "../unisynth-parity/parity.ts";
import { capturesDir, checkCapture } from "./capture.ts";

const ROOT = resolve(import.meta.dir, "../..");
export const PROFILE = "fast";
export const SONGS = ["atrium", "elevated", "receptor"] as const;
export const TOL: Tolerance = { ...DEFAULT_TOL, bandFloorDb: BAND_FLOOR_DB, onsetAlignment: MIN_ALIGNMENT };

export const srcDir = () => process.env.ELEVATED_SRC ?? join(homedir(), "Downloads/zenekk/TBC - Atrium");
export const refDir = () => process.env.ELEVATED_REF ?? "/tmp/elevated-ref";
/** Receptor's intro (its exe; build-ref.sh carves music.asm/music.h beside it). */
export const receptorDir = () => process.env.RECEPTOR_SRC ?? join(homedir(), "Downloads/zenekk/TBC-Receptor");

/** A song's music.asm (its music.h sits beside it). */
export function musicAsm(song: string): string {
  if (song === "atrium") return join(srcDir(), "music.asm");
  if (song === "elevated") return join(refDir(), "elevated-src/src/music.asm");
  if (song === "receptor") return join(receptorDir(), "music.asm");
  throw new Error(`unknown song ${song}`);
}

/** The reference render of a song, or of synth k alone. */
export const refWav = (song: string, solo?: number) =>
  join(refDir(), song, solo === undefined ? `${song}.wav` : `${song}-solo${solo}.wav`);

/** Both renders cut to the shorter one. */
export function cut(a: Audio, b: Audio): [Audio, Audio] {
  const n = Math.min(a.ch[0].length, b.ch[0].length);
  const c = (x: Audio): Audio => ({ rate: x.rate, ch: x.ch.map((ch) => ch.subarray(0, n)) });
  return [c(a), c(b)];
}

export const judge = (ours: Audio, ref: Audio): Report => compare(...cut(ours, ref), TOL);

export interface Row {
  name: string; // song, or song/solo k
  error?: string;
  report?: Report;
  capture?: string;
  capturePass?: boolean;
  warnings?: string[];
}

export const passes = (r: Row) => !r.error && !!r.report?.pass && r.capturePass !== false;

/** Songs whose sources and reference render are present; the rest are reported as skipped. */
export function available(names: string[]): { run: string[]; skipped: string[] } {
  const run: string[] = [], skipped: string[] = [];
  for (const n of names) (existsSync(musicAsm(n)) && existsSync(refWav(n)) ? run : skipped).push(n);
  return { run, skipped };
}

function sh(cmd: string[]): { ok: boolean; err: string } {
  const p = Bun.spawnSync(cmd, { cwd: ROOT, stdout: "pipe", stderr: "pipe" });
  return { ok: p.exitCode === 0, err: p.stderr.toString().trim() };
}

const bin = join(ROOT, "target", PROFILE, "synth-convert");
const render = join(ROOT, "target", PROFILE, "examples", "render");

function runOne(song: string, work: string, solo?: number): Row {
  const name = solo === undefined ? song : `${song}/solo${solo}`;
  const tag = solo === undefined ? song : `${song}-solo${solo}`;
  const conv = join(work, `${tag}.song`), ours = join(work, `${tag}_unisynth.wav`);
  const c = sh([bin, "elevated", musicAsm(song), conv, ...(solo === undefined ? [] : ["--solo", String(solo)])]);
  if (!c.ok) return { name, error: `convert: ${c.err.split("\n").pop()}` };
  const u = sh([render, conv, ours]);
  if (!u.ok) return { name, error: `render: ${u.err.slice(-300)}` };
  const row: Row = { name, warnings: c.err.split("\n").filter((l) => /warning/i.test(l)) };
  row.report = judge(readWav(ours), readWav(refWav(song, solo)));
  return row;
}

const f2 = (x: number) => (Number.isFinite(x) ? x.toFixed(2) : String(x));

export function formatReport(rows: Row[], skipped: string[]): string {
  const L = [
    "# Elevated parity (change add-elevated-import)",
    "",
    `Generated ${new Date().toISOString().slice(0, 10)} by \`bun tools/elevated-parity/parity.ts\`. Reference: the original synth.asm + music.asm (\`build-ref.sh\`); ours: \`synth-convert elevated\` → \`unisynth\` (\`--profile ${PROFILE}\`). Tolerances as tools/unisynth-parity's song parity: loudness 1 dB, octave bands 3 dB (bands ${BAND_FLOOR_DB} dB under the loudest skipped), timing by onset-strength alignment (correlation ≥ ${MIN_ALIGNMENT}); peak-picked onsets listed, not judged; renders cut to the shorter one. Songs and audio stay outside the repository (\`research/elevated-synth.md\`).`,
    "",
  ];
  if (skipped.length) L.push(`Skipped (sources or reference render absent): ${skipped.join(", ")}.`, "");
  L.push(`${rows.filter(passes).length} of ${rows.length} pass.`, "", "| render | result | loudness Δ dB | worst band Δ dB | alignment corr | capture | notes |", "|---|---|---:|---:|---|---|---|");
  for (const r of rows) {
    if (r.error || !r.report) {
      L.push(`| ${r.name} | ERROR | | | | | ${r.error ?? "no report"} |`);
      continue;
    }
    const rep = r.report;
    const worst = rep.bands.filter((b) => Number.isFinite(b.diff)).reduce((m, b) => (Math.abs(b.diff) > Math.abs(m.diff) ? b : m), rep.bands[0]);
    const cap = r.capturePass === undefined ? "" : r.capturePass ? "PASS" : "FAIL";
    const al = rep.onsets.corr === undefined ? "" : f2(rep.onsets.corr);
    L.push(`| ${r.name} | ${passes(r) ? "PASS" : "FAIL"} | ${f2(rep.loudness.diff)} | ${f2(worst.diff)} @ ${worst.fc} | ${al} | ${cap} | ${(r.warnings ?? []).length ? `${r.warnings!.length} converter warning(s)` : ""} |`);
  }
  return L.join("\n") + "\n";
}

function main() {
  const a = process.argv.slice(2);
  const flag = (k: string) => a.includes(k) && a.splice(a.indexOf(k), 1).length > 0;
  const opt = (k: string) => (a.includes(k) ? a.splice(a.indexOf(k), 2)[1] : undefined);
  const reportPath = opt("--report") ?? join(import.meta.dir, "report.md");
  const solos = flag("--solo"), capture = flag("--capture");
  const { run, skipped } = available(a.length ? a : [...SONGS]);
  if (!run.length) {
    console.log(`elevated parity: skipped (no sources or reference renders for ${skipped.join(", ")}; ELEVATED_SRC=${srcDir()}, ELEVATED_REF=${refDir()}; run build-ref.sh)`);
    process.exit(0);
  }
  const b = sh(["cargo", "build", "-q", "--profile", PROFILE, "-p", "synth-convert", "--bin", "synth-convert"]);
  const e = sh(["cargo", "build", "-q", "--profile", PROFILE, "-p", "unisynth", "--features", "std", "--example", "render"]);
  if (!b.ok || !e.ok) throw new Error(`cargo build failed:\n${b.err}\n${e.err}`);
  const work = join(refDir(), "parity");
  mkdirSync(work, { recursive: true });
  const rows: Row[] = [];
  for (const song of run) {
    const row = runOne(song, work);
    if (capture && !row.error) {
      const cap = join(capturesDir(), `${song}.wav`);
      if (existsSync(cap)) {
        const c = checkCapture(join(work, `${song}_unisynth.wav`), cap);
        [row.capture, row.capturePass] = [c.text, c.pass];
      }
    }
    rows.push(row);
    console.log(`${row.name}: ${passes(row) ? "PASS" : "FAIL"} ${row.error ?? ""}`);
    if (row.capture) console.log(row.capture);
    for (let k = 0; solos && existsSync(refWav(song, k)); k++) {
      const s = runOne(song, work, k);
      rows.push(s);
      console.log(`${s.name}: ${passes(s) ? "PASS" : "FAIL"} ${s.error ?? ""}`);
    }
  }
  writeFileSync(reportPath, formatReport(rows, skipped));
  process.exit(rows.every(passes) ? 0 : 1);
}

if (import.meta.main) main();
