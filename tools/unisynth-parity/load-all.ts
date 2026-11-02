// Load-all check of the format switch (change unify-v2-algorithms 6.9, spec "No song left behind"): every song we
// hold is in the v3 layout and loads in the new player (`song check`: validate, export, load without rendering).
//
//   bun tools/unisynth-parity/load-all.ts [--externals]      → tools/unisynth-parity/load-all.md, exit 1 on a failure
//
// Tracked locations fail on a song that does not load, is not in the current format, is skipped, or on a location
// holding fewer songs than the inventory records (design, "Song inventory"). Git-ignored locations are checked when
// present; with --externals the lab corpora are re-converted by this tree's synth-convert and checked (the stale
// `ws/unified` cache is rewritten). Absent locations are reported, never failed. SONG, CONVERT, SYNTH_LAB, GM_DLS,
// JOBS override the binaries, the lab, the bank and the parallelism.
import { existsSync, mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync } from "node:fs";
import { join, relative } from "node:path";
import { tmpdir } from "node:os";
import { fileURLToPath } from "node:url";
import { patchSong, templates } from "./songs-drift.ts";

const ROOT = fileURLToPath(new URL("../../", import.meta.url));
const SONG = process.env.SONG ?? join(ROOT, "target/release/song");
const CONVERT = process.env.CONVERT ?? join(ROOT, "target/release/synth-convert");
const LAB = process.env.SYNTH_LAB ?? join(process.env.HOME ?? "", "Downloads/64k-tools/synth-lab");
const BANK = process.env.GM_DLS ?? join(LAB, "ws/gm.dls");
const JOBS = Number(process.env.JOBS ?? 8);

/** One thing to load: a file to `song check`, or a source to convert first (`convert` = synth-convert arguments
 * before the output path). `format` is the format a JSON `file` must declare; `bad` a problem found while listing (a
 * patch or library in an old format). */
export type Job = { name: string; file?: string; json?: string; convert?: string[]; out?: string; format?: string; bad?: string };
export type Kind = "tracked" | "ignored" | "external";
export type Location = { name: string; kind: Kind; recorded: number; jobs: () => Job[] | null; note?: string };
export type Result = { name: string; ok: boolean; why?: string; known?: boolean };

/** Sources that never converted, before the switch either (the released V2 set skips them too): reported, not failed. */
export const UNCONVERTIBLE: Record<string, string> = {
  "v2songs/modland/KB--fr-013_flybye-loader.v2m": "the file is truncated (patches end past the data)",
};

const files = (dir: string, re: RegExp, deep = false): string[] => {
  if (!existsSync(dir)) return [];
  const out: string[] = [];
  for (const e of readdirSync(dir, { withFileTypes: true })) {
    const p = join(dir, e.name);
    if (e.isDirectory() && deep) out.push(...files(p, re, true));
    else if (e.isFile() && re.test(e.name)) out.push(p);
  }
  return out.sort();
};
const rel = (p: string) => relative(ROOT, p);
const music = (sub: string) => join(ROOT, "project/music", sub);
const songJobs = (dir: string) => files(dir, /\.song\.json$/).map((f) => ({ name: rel(f), file: f, format: "song/2" }));
const present = (dir: string, f: () => Job[]) => () => (existsSync(dir) ? f() : null);

/** A song that plays one library entry's devices (instrument, then its insert effects) for one note. */
function entrySong(devices: { kind: number; values: Record<string, number> }[]): string {
  const s = JSON.parse(patchSong({ kind: devices[0].kind, values: devices[0].values }));
  s.tracks[0].devices = devices;
  return JSON.stringify(s);
}

export function inventory(externals: boolean): Location[] {
  const probe = join(ROOT, "research/us-size-probe/out/songs");
  const lib = music(".library.json");
  const tbl = join(ROOT, "research/tbl-mini-cauldron/out");
  const v2 = join(LAB, "v2songs");
  const ext = (l: Location): Location => (externals ? l : { ...l, jobs: () => null, note: "opt-in: --externals" });
  return [
    { name: "project/music/*.song.json", kind: "tracked", recorded: 8, jobs: () => songJobs(music("")) },
    {
      name: "project/music/patches/*.patch.json", kind: "tracked", recorded: 2,
      jobs: () => files(music("patches"), /\.patch\.json$/).map((f) => {
        const p = JSON.parse(readFileSync(f, "utf8"));
        return { name: rel(f), json: patchSong(p), bad: p.format === "us-patch/2" ? undefined : `format ${p.format}, not us-patch/2` };
      }),
    },
    { name: "project/music/released/*.song.json", kind: "tracked", recorded: 28, jobs: () => songJobs(music("released")) },
    { name: "project/music/released/v2/*.song.json", kind: "tracked", recorded: 347, jobs: () => songJobs(music("released/v2")) },
    {
      name: "DAW instrument templates (ui/app/tracks.ts, main.ts)", kind: "tracked", recorded: 2,
      jobs: () => templates().map((t) => ({ name: t.slug, json: t.json })),
      note: "DAW test and bench fixtures are checked by the JS tests",
    },
    {
      name: "research/us-size-probe/out/songs/*.song", kind: "ignored", recorded: 27, // the table's 28 counted legacy/
      jobs: present(probe, () => files(probe, /\.song$/).map((f) => ({ name: rel(f), file: f }))),
      note: "re-converted by measure.sh",
    },
    {
      name: "research/us-size-probe/out/songs/legacy/*.song", kind: "ignored", recorded: 16,
      jobs: present(join(probe, "legacy"), () => files(join(probe, "legacy"), /\.song$/).map((f) => ({ name: rel(f), convert: ["ws", f] }))),
      note: "WaveSabre sources: converted, then loaded",
    },
    {
      name: "project/music/.library.json", kind: "ignored", recorded: 92 + 915,
      jobs: present(lib, () => {
        const l = JSON.parse(readFileSync(lib, "utf8"));
        if (l.format !== "us-library/2") return [{ name: rel(lib), bad: `format ${l.format}, not us-library/2 (rebuild it)` }];
        const tf = Object.entries(l.tunefish ?? {}).map(([n, e]: [string, any]) => ({ name: `tunefish program ${n}`, json: entrySong(e.devices) }));
        const ws = Object.entries(l.wavesabre ?? {}).flatMap(([f, es]: [string, any]) => es.map((e: any, i: number) => ({ name: `${f} #${i}`, json: entrySong(e.devices) })));
        return [...tf, ...ws];
      }),
      note: "one song per entry (instrument and its effects)",
    },
    {
      name: "research/tbl-mini-cauldron/out", kind: "ignored", recorded: 3,
      jobs: present(tbl, () => files(tbl, /\.song$/).map((f) => ({ name: rel(f), file: f }))),
      note: "re-converted by synth-convert xm / xm-phrase",
    },
    ext({
      name: "synth-lab/ws/*.bin + refs/*.bin (→ ws/unified)", kind: "external", recorded: 4 + 52,
      jobs: present(join(LAB, "ws"), () => [
        ...files(join(LAB, "ws"), /\.bin$/).map((f) => ({ name: relative(LAB, f), convert: ["ws", f] })),
        ...files(join(LAB, "ws/refs"), /\.bin$/).map((f) => ({ name: relative(LAB, f), convert: ["ws", f], out: join(LAB, "ws/unified", f.split("/").pop()!) })),
      ]),
      note: "ws/unified (52 old-layout conversions, read by nothing) rewritten from refs/",
    }),
    ext({
      name: "synth-lab/tunefish/media/tf4modules/*.tfm", kind: "external", recorded: 2,
      jobs: present(join(LAB, "tunefish/media/tf4modules"), () => files(join(LAB, "tunefish/media/tf4modules"), /\.tfm$/).map((f) => ({ name: relative(LAB, f), convert: ["tfm", f] }))),
    }),
    ...(["acceptance", "fr_public", "modland", "carved"] as const).map((d, i) => ext({
      name: `synth-lab/v2songs/${d}/**/*.v2m`, kind: "external", recorded: [15, 20, 325, 1][i],
      jobs: present(join(v2, d), () => files(join(v2, d), /\.v2m$/i, true).map((f) => ({ name: relative(LAB, f), convert: ["v2m", f] }))),
    })),
    ext({
      name: "synth-lab/v2songs/parity/**/*.song", kind: "external", recorded: 18,
      jobs: present(join(v2, "parity"), () => files(join(v2, "parity"), /\.song$/, true).map((f) => ({ name: relative(LAB, f), file: f }))),
      note: "conversions the V2 parity run rewrites each run (the 52 stale ones of an older layout, `parity/aux/`, moved to `unify/stale-parity-aux`)",
    }),
  ];
}

async function run(cmd: string[]): Promise<{ ok: boolean; out: string }> {
  const p = Bun.spawn(cmd, { stdout: "pipe", stderr: "pipe", env: { ...process.env, GM_DLS: existsSync(BANK) ? BANK : "" } });
  const [out, err] = [await new Response(p.stdout).text(), await new Response(p.stderr).text()];
  return { ok: (await p.exited) === 0, out: (out + err).trim() };
}

export async function check(job: Job, tmp: string, i: number): Promise<Result> {
  if (job.bad) return { name: job.name, ok: false, why: job.bad };
  let file = job.file;
  if (job.json !== undefined) {
    file = join(tmp, `${i}.song.json`);
    writeFileSync(file, job.json);
  }
  if (job.convert) {
    file = job.out ?? join(tmp, `${i}.song`);
    const c = await run([CONVERT, ...job.convert, file]);
    if (!c.ok && UNCONVERTIBLE[job.name]) return { name: job.name, ok: true, known: true, why: UNCONVERTIBLE[job.name] };
    if (!c.ok) return { name: job.name, ok: false, why: `conversion: ${c.out.split("\n").pop()}` };
  }
  if (job.format && job.file) {
    const f = JSON.parse(readFileSync(job.file, "utf8")).format;
    if (f !== job.format) return { name: job.name, ok: false, why: `format ${f}, not ${job.format}` };
  }
  const r = await run([SONG, "check", file!]);
  return r.ok ? { name: job.name, ok: true } : { name: job.name, ok: false, why: r.out.split("\n").pop() };
}

async function pool<T, R>(xs: T[], n: number, f: (x: T, i: number) => Promise<R>): Promise<R[]> {
  const out: R[] = new Array(xs.length);
  let next = 0;
  await Promise.all(Array.from({ length: Math.min(n, xs.length) }, async () => {
    while (next < xs.length) {
      const i = next++;
      out[i] = await f(xs[i], i);
    }
  }));
  return out;
}

async function main() {
  const externals = process.argv.includes("--externals");
  const tmp = mkdtempSync(join(tmpdir(), "load-all-"));
  const rows: string[] = [];
  const failures: string[] = [];
  const notes: string[] = [];
  let bad = false;
  try {
    for (const loc of inventory(externals)) {
      const jobs = loc.jobs();
      if (!jobs) {
        rows.push(`| ${loc.name} | ${loc.kind} | ${loc.recorded} | – | – | absent${loc.note?.startsWith("opt-in") ? " (opt-in: --externals)" : ""} |`);
        continue;
      }
      const res = await pool(jobs, JOBS, (j, i) => check(j, tmp, i + rows.length * 100_000));
      const failed = res.filter((r) => !r.ok);
      const known = res.filter((r) => r.known);
      const short = jobs.length < loc.recorded;
      const status = failed.length || short ? "**FAIL**" : "ok";
      if (status !== "ok") bad = true;
      const loaded = res.length - failed.length - known.length;
      rows.push(`| ${loc.name} | ${loc.kind} | ${loc.recorded} | ${jobs.length} | ${loaded}${known.length ? ` (+${known.length} unconvertible source)` : ""} | ${status}${short ? ` (fewer than recorded)` : ""}${loc.note ? `; ${loc.note}` : ""} |`);
      notes.push(...known.map((r) => `- \`${r.name}\`: ${r.why}`));
      failures.push(...failed.map((r) => `- ${loc.name}: \`${r.name}\`: ${r.why}`));
      console.error(`${loc.name}: ${res.length - failed.length}/${jobs.length}${failed.length ? ` (${failed.length} failed)` : ""}`);
    }
  } finally {
    rmSync(tmp, { recursive: true, force: true });
  }
  const md = [
    `# Load-all check (${new Date().toISOString().slice(0, 10)})`,
    "",
    "Change unify-v2-algorithms 6.9 (spec \"No song left behind\"): `bun tools/unisynth-parity/load-all.ts" + (externals ? " --externals" : "") + "`.",
    "Every song is validated, exported and loaded into the song player (`song check`); JSON files must declare `song/2`",
    "(patches `us-patch/2`, the library `us-library/2`). Sources are converted by this tree's `synth-convert` first.",
    "",
    "| Location | Kind | Recorded | Found | Loaded | Status |",
    "|---|---|---|---|---|---|",
    ...rows,
    "",
    failures.length ? "## Failures\n\n" + failures.join("\n") : bad ? "Every song found loads; a location holds fewer than recorded." : "No failures.",
    ...(notes.length ? ["", "## Sources that never converted (reported, `UNCONVERTIBLE`)", "", ...notes] : []),
    "",
  ].join("\n");
  writeFileSync(join(ROOT, "tools/unisynth-parity/load-all.md"), md);
  console.log(md);
  process.exit(bad ? 1 : 0);
}

if (import.meta.main) await main();
