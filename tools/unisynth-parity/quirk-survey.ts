// Quirk survey (change unify-v2-algorithms, task 6.3): for each quirk candidate of `params::era_settings`, build the
// renderer with it cleared (`--cfg us_quirk_off="<quirk>"`), render every tracked V2 song that sets it, and compare:
// against its quirk-on render (the task 6.2 profiles) and, for the parity songs, against the original V2 render.
// A quirk is kept when clearing it takes a song outside the parity tolerance (1 dB loudness, 3 dB per band; the
// user's rule, design "Cycle 3"); songs that only cross the D7.3 drift thresholds are listed for listening.
//
//   bun tools/unisynth-parity/quirk-survey.ts --against <songs-base dir> --out <dir> [--only <quirk>]
//   Writes <dir>/<quirk>.json (rows) and <dir>/quirks.md (the table). V2 references: $V2_SONGS/ref.
import { existsSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { compare, DEFAULT_TOL, readWav } from "../synth-parity/metrics.ts";
import { FLAG } from "./drift.ts";
import { BAND_FLOOR_DB } from "./parity.ts";

const ROOT = fileURLToPath(new URL("../../", import.meta.url));
const V2_DIR = join(ROOT, "project/music/released/v2");
const REF = join(process.env.V2_SONGS ?? join(process.env.HOME ?? "", "Downloads/64k-tools/synth-lab/v2songs"), "ref");

/** Each candidate: what it reproduces and the format versions that set it (2004 build of version 5 excepted). The
 * 2026-11-01 survey dropped `boxed_osc` and `dc_bias` (no song moved past even the listening thresholds without
 * them); their numbers are in docs/synth/unisynth.md. */
export const QUIRKS: { name: string; what: string; sets: (v: number) => boolean }[] = [
  { name: "coarse_env", what: "256-sample control frame and the year-2000 envelope laws", sets: (v) => v < 2 },
  { name: "lcg_noise", what: "the MSVC LCG noise generator", sets: (v) => v < 5 },
  { name: "exact_math", what: "exact sine and arctangent instead of the fast approximations", sets: (v) => v < 6 },
  { name: "integer_fm", what: "FM phase offset rounded to an integer (x87 fistp, out of range collapses the carrier)", sets: (v) => v < 6 },
  { name: "fixed_seed", what: "noise seeds start at 0", sets: (v) => v < 6 },
];

/** Parity songs with an original render: page slug → reference file. */
const PARITY: Record<string, string> = {
  ...Object.fromEntries(["fr08", "flybye", "fr014", "farbomat", "fr022_party", "fr019", "brullwurfel", "candytron", "fr022_final", "fr024", "fr027", "fr029", "kkrieger", "pzero_new", "v2_zeitmaschine_new"].map((s) => [`v2-${s.replaceAll("_", "-")}`, `${s}.wav`])),
  "debris-ost": "werkkzeug3__data__debris__debris_ost.wav",
  "kb-fr-052-platinum": "KB--fr-052_platinum.wav",
  "jandor-stars": "Jandor--stars.wav",
};

/** The .v2m format version a converted song's meta states. */
export function versionOf(description: string): number | undefined {
  const m = /format version (\d)\)/.exec(description);
  return m ? +m[1] : undefined;
}

async function sh(cmd: string[], env: Record<string, string> = {}): Promise<string> {
  const p = Bun.spawn(cmd, { cwd: ROOT, env: { ...process.env, ...env }, stdout: "pipe", stderr: "pipe" });
  const [out, err] = await Promise.all([new Response(p.stdout).text(), new Response(p.stderr).text()]);
  if ((await p.exited) !== 0) throw new Error(`${cmd.join(" ")}: ${err.trim().split("\n").slice(-4).join(" | ")}`);
  return out;
}

async function main() {
  const arg = (k: string) => { const i = process.argv.indexOf(k); return i > 0 ? process.argv[i + 1] : undefined; };
  const against = arg("--against"), out = arg("--out"), only = arg("--only");
  if (!against || !out) throw new Error("usage: quirk-survey.ts --against <songs-base dir> --out <dir> [--only <quirk>]");
  mkdirSync(out, { recursive: true });
  const songs = [...new Bun.Glob("*.song.json").scanSync(V2_DIR)].map((f) => {
    const d = JSON.parse(readFileSync(join(V2_DIR, f), "utf8"));
    return { file: `project/music/released/v2/${f}`, slug: f.replace(/\.song\.json$/, ""), v: versionOf(d.meta?.description ?? "") };
  });
  const unknown = songs.filter((s) => s.v === undefined);
  if (unknown.length) throw new Error(`no format version in the meta of ${unknown.map((s) => s.file).join(", ")}`);

  for (const q of QUIRKS.filter((q) => !only || q.name === only)) {
    const set = songs.filter((s) => q.sets(s.v!));
    console.log(`${q.name}: ${set.length} songs set it`);
    if (!set.length) { writeFileSync(join(out, `${q.name}.json`), JSON.stringify({ quirk: q.name, rows: [], parity: [] })); continue; }
    const target = join(ROOT, "target/quirk", q.name);
    await sh(["cargo", "build", "-q", "--release", "-p", "song"], { CARGO_TARGET_DIR: target, RUSTFLAGS: `--cfg us_quirk_off="${q.name}"` });
    const song = join(target, "release/song");
    const rowsFile = join(out, `${q.name}.rows.json`);
    const esc = (s: string) => s.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
    await sh(["bun", "tools/unisynth-parity/songs-drift.ts", "--against", against, "--json", rowsFile], { SONG: song, ONLY: `^(${set.map((s) => esc(s.file)).join("|")})$` });
    const rows = JSON.parse(readFileSync(rowsFile, "utf8"));
    // the parity songs that set it: quirk-off render against the original
    const parity: { slug: string; loudness: number; band: number; pass: boolean }[] = [];
    for (const s of set.filter((s) => PARITY[s.slug] && existsSync(join(REF, PARITY[s.slug])))) {
      const wav = join(out, `${q.name}-${s.slug}.wav`);
      await sh([song, "render", join(ROOT, s.file), "-o", wav, "--threads", "4"]);
      const a = readWav(wav), b = readWav(join(REF, PARITY[s.slug]));
      const n = Math.min(a.ch[0].length, b.ch[0].length);
      const cut = (x: typeof a) => ({ rate: x.rate, ch: x.ch.map((c) => c.subarray(0, n)) });
      const r = compare(cut(a), cut(b), { ...DEFAULT_TOL, bandFloorDb: BAND_FLOOR_DB });
      const worst = r.bands.reduce((m, x) => (Math.abs(x.diff) > Math.abs(m) ? x.diff : m), 0);
      parity.push({ slug: s.slug, loudness: r.loudness.diff, band: worst, pass: r.loudness.pass && r.bands.every((x) => x.pass) });
      rmSync(wav, { force: true });
    }
    writeFileSync(join(out, `${q.name}.json`), JSON.stringify({ quirk: q.name, rows, parity }));
  }

  // the table, from every quirk's result on disk
  const lines = [
    "| Quirk | Reproduces | Versions | Songs that set it | Outside parity tolerance without it | Past D7.3 (listen) | Parity songs failing | Keep |",
    "|---|---|---|---:|---|---:|---|---|",
  ];
  for (const q of QUIRKS) {
    const f = join(out, `${q.name}.json`);
    if (!existsSync(f)) continue;
    const { rows, parity } = JSON.parse(readFileSync(f, "utf8"));
    const outside = rows.filter((r: any) => Math.abs(r.loudness) > DEFAULT_TOL.loudnessDb || Math.abs(r.band.diff) > DEFAULT_TOL.bandDb || r.note);
    const listen = rows.filter((r: any) => Math.abs(r.loudness) > FLAG.loudnessDb || Math.abs(r.band.diff) > FLAG.bandDb);
    const failing = parity.filter((p: any) => !p.pass);
    const keep = outside.length > 0 || failing.length > 0;
    const vs = [0, 1, 2, 3, 4, 5, 6].filter(q.sets).join(", ");
    const names = (xs: any[]) => xs.slice(0, 6).map((r: any) => `${(r.file ?? r.slug).split("/").pop().replace(".song.json", "")} (${r.loudness.toFixed(2)} dB, ${(r.band?.diff ?? r.band).toFixed(2)} dB)`).join("; ") + (xs.length > 6 ? `; +${xs.length - 6}` : "");
    lines.push(`| \`${q.name}\` | ${q.what} | ${vs} | ${rows.length} | ${outside.length ? `${outside.length}: ${names(outside)}` : "none"} | ${listen.length} | ${failing.length ? names(failing) : "none"} | ${keep ? "**keep**" : "drop"} |`);
  }
  writeFileSync(join(out, "quirks.md"), lines.join("\n") + "\n");
  console.log(lines.join("\n"));
}

if (import.meta.main) await main();
