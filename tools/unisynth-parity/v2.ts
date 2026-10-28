// V2 parity (change add-v2-import, task 6.2): the V2 acceptance songs, rendered by v2redux (the reference) and,
// converted by `synth-convert v2m`, by `unisynth`, compared with the song settings of `parity.ts` (1 dB loudness, 3 dB
// per octave band 63 Hz–16 kHz with the 24 dB floor, onset-strength alignment). Renders cut to the shorter one (the
// reference rings its tail out). Writes `v2-report.md` (metrics, never audio) and, with `--renders`, the unified
// renders to `renders/unisynth/<song>_unisynth.{wav,mp3}` (git-ignored).
//
//   bun tools/unisynth-parity/v2.ts [--report path] [--renders] [--reuse] [song…]
//     V2_LAB / V2REDUX / V2_SONGS as in tools/v2-extract/v2.ts
// Exit 0 when every song that ran passed (or the songs were absent and skipped), 1 on a failing song.
import { copyFileSync, existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";
import { compare, DEFAULT_TOL, readWav, type Audio, type Report } from "../synth-parity/metrics.ts";
import { ACCEPTANCE, acceptancePath, reference, SONGS, V2REDUX } from "../v2-extract/v2.ts";
import { BAND_FLOOR_DB, MIN_ALIGNMENT, PROFILE } from "./parity.ts";

const ROOT = resolve(import.meta.dir, "../..");

export interface V2Row {
  name: string;
  version?: number;
  error?: string;
  report?: Report;
  /** Seconds compared, and the unified render time on `PROFILE`. */
  secs?: number;
  ourTime?: number;
  warnings?: string[];
}

export interface V2Run {
  rows: V2Row[];
  skipped?: string;
  signoff: { decision: "approved" | "findings" | "pending"; by?: string; date?: string; notes?: string };
  generated: string;
}

export const passes = (r: V2Row) => !r.error && !!r.report?.pass;

/** Corpus songs (not acceptance songs) that exercise the aux buses (task 5.2), relative to `$V2_SONGS`; skipped when absent. */
export const AUX_SONGS: Record<string, string> = {
  debris_ost: "fr_public/werkkzeug3__data__debris__debris_ost.v2m",
  "fr-052_platinum": "modland/KB--fr-052_platinum.v2m",
  stars: "modland/Jandor--stars.v2m",
};

/** The `.v2m` of a song: an acceptance song, or one of [`AUX_SONGS`]. */
export const songPath = (name: string) => (AUX_SONGS[name] ? join(SONGS, AUX_SONGS[name]) : acceptancePath(name));

/** Both renders cut to the shorter one. */
export function cut(a: Audio, b: Audio): [Audio, Audio] {
  const n = Math.min(a.ch[0].length, b.ch[0].length);
  const c = (x: Audio): Audio => ({ rate: x.rate, ch: x.ch.map((ch) => ch.subarray(0, n)) });
  return [c(a), c(b)];
}

export function judge(ours: Audio, ref: Audio): Report {
  const [a, b] = cut(ours, ref);
  return compare(a, b, { ...DEFAULT_TOL, bandFloorDb: BAND_FLOOR_DB, onsetAlignment: MIN_ALIGNMENT });
}

const f2 = (x: number) => (Number.isFinite(x) ? x.toFixed(2) : String(x));

export function formatRun(run: V2Run): string {
  const L = [
    "# V2 parity (change add-v2-import)",
    "",
    `Generated ${run.generated} by \`bun tools/unisynth-parity/v2.ts\`. Reference: v2redux (\`v2dump\`, the song's own format version); ours: \`synth-convert v2m\` → \`unisynth\` (\`--profile ${PROFILE}\`). Metrics as the reference songs of \`report.md\`: loudness within 1 dB, octave bands within 3 dB (bands 24 dB under the loudest skipped), onset-strength alignment (lag within 100 frames, correlation ≥ ${MIN_ALIGNMENT}); renders cut to the shorter one. Songs and audio stay outside the repository (\`research/v2-intros.md\`).`,
    "",
  ];
  if (run.skipped) L.push(`Skipped: ${run.skipped}.`, "");
  const section = (rows: V2Row[]) => {
  L.push(`${rows.filter(passes).length} of ${rows.length} pass.`, "", "| song | ver | result | loudness Δ dB | worst band Δ dB | alignment corr | × real time | notes |", "|---|---:|---|---:|---:|---:|---:|---|");
  for (const r of rows) {
    if (r.error || !r.report) {
      L.push(`| ${r.name} | ${r.version ?? ""} | ERROR | | | | | ${r.error ?? "no report"} |`);
      continue;
    }
    const rep = r.report;
    const worst = rep.bands.filter((b) => Number.isFinite(b.diff)).reduce((m, b) => (Math.abs(b.diff) > Math.abs(m.diff) ? b : m), rep.bands[0]);
    const rt = r.secs && r.ourTime ? (r.secs / r.ourTime).toFixed(1) : "";
    L.push(
      `| ${r.name} | ${r.version ?? ""} | ${rep.pass ? "PASS" : "FAIL"} | ${f2(rep.loudness.diff)} | ${f2(worst.diff)} @ ${worst.fc} | ${rep.onsets.corr === undefined ? "" : f2(rep.onsets.corr)} | ${rt} | ${(r.warnings ?? []).length ? `${r.warnings!.length} converter warning(s)` : ""} |`,
    );
  }
  };
  section(run.rows.filter((r) => !AUX_SONGS[r.name]));
  const aux = run.rows.filter((r) => AUX_SONGS[r.name]);
  if (aux.length) {
    L.push("", "## Aux buses (corpus songs)", "", "Songs outside the acceptance set that use V2's aux A/B buses (aux oscillators, aux receives).", "");
    section(aux);
  }
  L.push("", "## A/B listening", "", run.signoff.decision === "pending" ? "Pending: the acceptance songs are rendered both ways (`--renders`) for the user to compare." : `Decision: **${run.signoff.decision}**${run.signoff.by ? ` by ${run.signoff.by}` : ""}${run.signoff.date ? ` on ${run.signoff.date}` : ""}.`);
  if (run.signoff.notes) L.push("", run.signoff.notes);
  return L.join("\n") + "\n";
}

function sh(cmd: string[]): { ok: boolean; out: string; err: string } {
  const p = Bun.spawnSync(cmd, { cwd: ROOT, stdout: "pipe", stderr: "pipe" });
  return { ok: p.exitCode === 0, out: p.stdout.toString(), err: p.stderr.toString().trim() };
}

const bin = (n: string) => join(ROOT, "target", PROFILE, n);
const ex = (n: string) => join(ROOT, "target", PROFILE, "examples", n);

/** Reference → convert → render → compare, for one acceptance song. */
export function runSong(name: string, work: string, reuse = false): V2Row {
  const row: V2Row = { name };
  const v2m = songPath(name);
  const conv = join(work, `${name}.song`), ours = join(work, `${name}_unisynth.wav`), cache = join(work, `${name}.json`);
  try {
    const ref = reference(v2m);
    if (reuse && existsSync(cache) && existsSync(ours)) Object.assign(row, JSON.parse(readFileSync(cache, "utf8")));
    else {
      const c = sh([bin("synth-convert"), "v2m", v2m, conv, "4"]);
      if (!c.ok) return { ...row, error: `convert: ${c.err.split("\n").pop()}` };
      row.warnings = c.err.split("\n").filter((l) => /warning/i.test(l));
      const u = sh([ex("render"), conv, ours]);
      if (!u.ok) return { ...row, error: `render: ${u.err.slice(-300)}` };
      row.ourTime = Number(/rendered in ([\d.]+) s/.exec(u.err)?.[1]);
      writeFileSync(cache, JSON.stringify({ ourTime: row.ourTime, warnings: row.warnings }));
    }
    const a = readWav(ours), r = readWav(ref);
    row.secs = Math.min(a.ch[0].length, r.ch[0].length) / a.rate;
    row.report = judge(a, r);
  } catch (e) {
    return { ...row, error: String((e as Error).message) };
  }
  return row;
}

function main() {
  const a = process.argv.slice(2);
  const flag = (k: string) => a.includes(k) && a.splice(a.indexOf(k), 1).length > 0;
  const opt = (k: string) => (a.includes(k) ? a.splice(a.indexOf(k), 2)[1] : undefined);
  const reportPath = opt("--report") ?? join(import.meta.dir, "v2-report.md");
  const renders = flag("--renders"), reuse = flag("--reuse");
  const names = a.length ? a : [...ACCEPTANCE, ...Object.keys(AUX_SONGS)];
  const signoffPath = join(import.meta.dir, "v2-signoff.json");
  const signoff = existsSync(signoffPath) ? JSON.parse(readFileSync(signoffPath, "utf8")) : { decision: "pending" };
  const run: V2Run = { rows: [], signoff, generated: new Date().toISOString().slice(0, 10) };
  const missing = names.filter((n) => !existsSync(songPath(n)));
  if (!existsSync(join(V2REDUX, "build/v2dump")) || missing.length === names.length) {
    run.skipped = `the V2 songs or v2redux are absent (V2_SONGS=${SONGS}, V2REDUX=${V2REDUX}; run tools/v2-extract/v2.ts gather)`;
    console.log(`skip: ${run.skipped}`);
    process.exit(0);
  }
  if (missing.length) run.skipped = `${missing.join(", ")} not found in ${SONGS}`;
  const b = sh(["cargo", "build", "-q", "--profile", PROFILE, "-p", "synth-convert", "--bin", "synth-convert"]);
  const e = sh(["cargo", "build", "-q", "--profile", PROFILE, "-p", "unisynth", "--features", "std", "--example", "render"]);
  if (!b.ok || !e.ok) throw new Error(`cargo build failed:\n${b.err}\n${e.err}`);
  const work = join(SONGS, "parity");
  mkdirSync(work, { recursive: true });
  for (const n of names.filter((x) => !missing.includes(x))) {
    const row = runSong(n, work, reuse);
    row.version = JSON.parse(Bun.spawnSync([join(SONGS, ".bin/v2dumpjson"), songPath(n)]).stdout.toString() || "{}").version;
    run.rows.push(row);
    console.log(`${n}: ${passes(row) ? "PASS" : "FAIL"} ${row.error ?? ""}`);
    if (renders && !row.error) {
      const dir = join(ROOT, "renders/unisynth");
      mkdirSync(dir, { recursive: true });
      const wav = join(dir, `${n}_unisynth.wav`);
      copyFileSync(join(work, `${n}_unisynth.wav`), wav);
      Bun.spawnSync(["ffmpeg", "-y", "-loglevel", "error", "-i", wav, "-b:a", "192k", wav.replace(/\.wav$/, ".mp3")]);
    }
  }
  if (!a.length) writeFileSync(reportPath, formatRun(run));
  process.exit(run.rows.every(passes) ? 0 : 1);
}

if (import.meta.main) main();
