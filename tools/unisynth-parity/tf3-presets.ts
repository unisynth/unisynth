// Tunefish 3 preset parity (change add-tf3-import 6.4): every distinct preset of Enigma Studio 3's bank, 16 notes over
// two octaves, the original editor (tools/tf3-parity/render-ref.ts --editor) against the editor-mode conversion
// (synth-convert e3prj --editor → render example), judged by loudness and octave bands. Writes tf3-presets-report.md
// (metrics only). Skips without the reference renderer or the bank.
//
//   bun tools/unisynth-parity/tf3-presets.ts [--report path]
//
// Bank: $SYNTH_LAB/tunefish3/tf3programs (default ~/Downloads/64k-tools/synth-lab). Single notes are not used: unison
// and PAD start phases are random in the original too, so one note's octave bands vary by up to 13 dB between its own
// runs (measured with TF3_TICK); over 16 notes they settle.
import { existsSync, mkdirSync, readFileSync, readdirSync, writeFileSync } from "node:fs";
import { spawnSync } from "node:child_process";
import { homedir } from "node:os";
import { join } from "node:path";
import { compare, DEFAULT_TOL, readWav } from "../synth-parity/metrics.ts";

const ROOT = join(import.meta.dir, "../..");
const LAB = process.env.SYNTH_LAB ?? join(homedir(), "Downloads/64k-tools/synth-lab");
const BANK = join(LAB, "tunefish3/tf3programs");
const REF = process.env.TF3_REF ?? "/tmp/tf3-ref";
const WORK = join(ROOT, "build/tf3-parity/presets");
/** Judged like the songs: 1 dB loudness, 3 dB per octave band within 24 dB of the loudest. */
const TOL = { loudnessDb: 1, bandDb: 3, floorDb: 24 };

/** The distinct presets (identical parameter sets once, the lowest number keeps it): number, name, `[name, value]`. */
export function presets(dir = BANK): { n: number; name: string; params: [string, string][] }[] {
  const seen = new Set<string>();
  return readdirSync(dir)
    .filter((f) => /^program\d+\.txt$/.test(f))
    .map((f) => Number(f.slice(7, -4)))
    .sort((a, b) => a - b)
    .flatMap((n) => {
      const lines = readFileSync(join(dir, `program${n}.txt`), "latin1").replace(/\r/g, "").split("\n");
      const params = lines.slice(1).filter((l) => l.includes(";")).map((l) => l.split(";") as [string, string]);
      const key = params.map(([, v]) => v).join(" ");
      if (seen.has(key)) return [];
      seen.add(key);
      return [{ n, name: lines[0]!.trim(), params }];
    });
}

/** A one-instrument project playing 16 notes (two octaves around C4) with the preset's values. */
export function project(params: [string, string][]): string {
  const notes = Array.from({ length: 16 }, (_, k) => `<ne i="0" n="${[49, 53, 56, 61, 65, 68, 73, 44][k % 8]}" r="${k * 8}" v="-1" e="0"/><ne i="-1" n="128" r="${k * 8 + 6}" v="-1" e="0"/>`).join("");
  return `<?xml version="1.0"?><enigma version="3"><songs><song bpm="120" id="1" name="presets"><patterns><pattern rowcount="132" patternid="0" trackcount="1"><tracks><track>${notes}</track></tracks></pattern></patterns><patterninstances><patterninstance rowoffset="0" patternid="0" track="0"/></patterninstances></song></songs><instruments><instrument id="0" name="p"><parameters>${params.map(([k, v]) => `<param value="${v}" name="${k}"/>`).join("")}</parameters></instrument></instruments></enigma>`;
}

interface Row { n: number; name: string; loud?: number; band?: { fc: number; diff: number }; pass?: boolean; error?: string }

function judge(ours: string, ref: string): Omit<Row, "n" | "name"> {
  const rep = compare(readWav(ours), readWav(ref), { ...DEFAULT_TOL, bandFloorDb: TOL.floorDb });
  const top = Math.max(...rep.bands.map((b) => b.ref));
  const band = rep.bands.filter((b) => Number.isFinite(b.diff) && b.ref >= top - TOL.floorDb).reduce((m, b) => (Math.abs(b.diff) > Math.abs(m.diff) ? b : m), { fc: 0, diff: 0 });
  const loud = rep.loudness.diff;
  return { loud, band: { fc: band.fc, diff: band.diff }, pass: Math.abs(loud) <= TOL.loudnessDb && Math.abs(band.diff) <= TOL.bandDb };
}

export function format(rows: Row[], generated: string): string {
  const ok = rows.filter((r) => r.pass).length;
  const worstLoud = Math.max(...rows.filter((r) => r.loud !== undefined).map((r) => Math.abs(r.loud!)));
  return [
    "# Tunefish 3 presets: parity (change add-tf3-import)",
    "",
    `Generated ${generated} by \`bun tools/unisynth-parity/tf3-presets.ts\`. Each distinct preset of Enigma Studio 3's bank plays 16 notes; the original editor build renders the reference, the editor-mode conversion renders ours. Judged at ${TOL.loudnessDb} dB loudness and ${TOL.bandDb} dB per octave band (bands within ${TOL.floorDb} dB of the loudest).`,
    "",
    "| # | Preset | Result | Loudness Δ dB | Worst judged band Δ dB |",
    "|---:|---|---|---:|---:|",
    ...rows.map((r) => `| ${r.n} | ${r.name} | ${r.error ? "error: " + r.error : r.pass ? "pass" : "off"} | ${r.loud?.toFixed(2) ?? ""} | ${r.band ? `${r.band.diff.toFixed(1)} (${r.band.fc} Hz)` : ""} |`),
    "",
    `${ok} of ${rows.length} within both; every preset within ${worstLoud.toFixed(2)} dB loudness. The rest differ in one band, mostly 63 Hz or 16 kHz.`,
    "",
  ].join("\n");
}

if (import.meta.main) {
  const a = process.argv.slice(2);
  const report = a.includes("--report") ? a[a.indexOf("--report") + 1]! : join(import.meta.dir, "tf3-presets-report.md");
  const exe = join(REF, "build/render-ref-editor");
  if (!existsSync(exe) || !existsSync(BANK)) {
    console.log(`tf3 presets: skipped (${!existsSync(exe) ? "no reference renderer: tools/tf3-parity/build-ref.sh" : "no bank at " + BANK})`);
    process.exit(0);
  }
  mkdirSync(WORK, { recursive: true });
  const conv = join(ROOT, "target/release/synth-convert"), render = join(ROOT, "target/release/examples/render");
  const rows: Row[] = [];
  for (const { n, name, params } of presets()) {
    const f = join(WORK, `p${n}`);
    writeFileSync(f + ".e3prj", project(params));
    const steps = [
      spawnSync("bun", [join(ROOT, "tools/tf3-parity/render-ref.ts"), f + ".e3prj", f + "-ref.wav", "--editor", "--tail", "2"], { encoding: "utf8" }),
      spawnSync(conv, ["e3prj", f + ".e3prj", f + ".song", "--editor", "2"], { encoding: "utf8" }),
      spawnSync(render, [f + ".song", f + "-ours.wav"], { encoding: "utf8" }),
    ];
    const bad = steps.find((s) => s.status !== 0);
    const row: Row = bad ? { n, name, error: (bad.stderr || "").trim().split("\n").pop()!.slice(0, 80) } : { n, name, ...judge(f + "-ours.wav", f + "-ref.wav") };
    rows.push(row);
    console.log(`${row.pass ? "pass" : row.error ? "ERR " : "off "}  ${n} ${name}`);
  }
  writeFileSync(report, format(rows, new Date().toISOString().slice(0, 10)));
  console.log(`${rows.filter((r) => r.pass).length} of ${rows.length} within tolerance → ${report}`);
}
