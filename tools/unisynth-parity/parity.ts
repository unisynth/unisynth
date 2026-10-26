// Parity of the unified synth against the legacy engines (design D11): every legacy device test song, plus the six
// reference songs from the synth lab, is rendered by its legacy crate and, converted by `synth-convert`, by `unisynth`,
// then compared with the shared metrics (tools/synth-parity: 1 dB integrated loudness, 3 dB per octave band 63 Hz-16 kHz,
// onsets within 100 frames for WaveSabre, one 512-frame block for Tunefish). Writes `report.md` (metrics, never audio).
//
//   bun tools/unisynth-parity/parity.ts [--no-songs] [--report path] [--reuse dir]
//     WS_SONG_DIR  dir with the WaveSabre reference songs (groove.bin elysian.bin engage.bin eladamri.bin [gm.dls])
//     TF_SONGS     dir with the Tunefish .tfm reference songs
//     GM_DLS       bank for the Adultery/Specimen-bank songs (default $WS_SONG_DIR/gm.dls, else the macOS bank)
// Exit 0 when everything that ran passed (or the songs were absent and skipped), 1 on a failing item.
import { existsSync, mkdirSync, readFileSync, readdirSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";
import { compare, DEFAULT_TOL, loudness, octaveLtas, readWav, type Report, type Tolerance } from "../synth-parity/metrics.ts";

const ROOT = resolve(import.meta.dir, "../..");
export const MACOS_BANK = "/System/Library/Components/CoreAudio.component/Contents/Resources/gs_instruments.dls";
export const ONSET_FRAMES = { ws: DEFAULT_TOL.onsetFrames, tf: 512 } as const;
/**
 * Bands more than this far below the loudest reference band are not compared: next to it they are inaudible, and
 * between two different engines they hold leakage, envelope-edge sub-bass and the 16-bit noise floor rather than the
 * sound. The same cut the in-process parity tests use (`crates/synth-convert/tests/common/metrics.rs`).
 */
export const BAND_FLOOR_DB = 24;
/** Least correlation of the onset-strength curves (lowest seen across the passing renders: 0.57). */
export const MIN_ALIGNMENT = 0.5;
/** Real-time factors the report must show for the reference songs (task 7.4). */
export const SPEED_FLOOR: Record<string, number> = { elysian: 5, "no-remorse": 10 };

export type Engine = "ws" | "tf";
export type Level = "device" | "song";

export interface Row {
  level: Level;
  name: string;
  engine: Engine;
  /** Set when the pipeline itself failed (convert error, silent render ...). */
  error?: string;
  report?: Report;
  /** Seconds of audio, legacy render seconds, unified render seconds. */
  secs?: number;
  legacyTime?: number;
  ourTime?: number;
  /** The same two timings on the shipped size profile (`release`, opt-level z), for the songs with a speed floor. */
  sizeBuild?: { legacyTime: number; ourTime: number };
  warnings?: string[];
  /** Set when the item was judged on several random seeds: how much its numbers move with them. */
  ensemble?: { seeds: number; loudnessSd: number; bandSd: number[]; why: string };
}

export interface Signoff {
  decision: "approved" | "findings" | "pending";
  by?: string;
  date?: string;
  notes?: string;
}

export interface Run {
  rows: Row[];
  /** Why the reference songs did not run, when they did not. */
  skipped?: string;
  signoff: Signoff;
  sizes?: string;
  generated: string;
}

// ---------- verdicts ----------

/**
 * Items whose sound is decided by random draws the engines make differently (the legacy LCG with its fixed seed, our
 * per-voice xorshift): up to 10 unison layers start at random phases and sum coherently or not, and a song of 2 ms
 * noise bursts is a few thousand random numbers. One draw of each says little (seed to seed ours moves by up to several
 * dB in the lowest bands of Global Groove), so these are judged on `SEEDS` draws of ours: the mean must agree with the
 * reference within the usual tolerance plus twice the spread between seeds.
 */
/**
 * Items that sound different from the legacy render on purpose and have a wider tolerance: a filter set to its open end
 * (cutoff at the top, no resonance) is passed through, not run (`flt::transparent`), while the legacy filter rolls the top
 * octaves off (the state-variable low-pass: −1.6 dB at 10 kHz, −4 dB at 16 kHz). Global Groove is a song of nothing but
 * noise through such a filter: hi-hat energy above 8 kHz is where the bypass brightens it.
 */
export const ACCEPTED: Record<string, { loudnessDb: number; bandDb: number; why: string }> = {
  "global-groove": { loudnessDb: 2.5, bandDb: 4, why: "noise through a low-pass at the top of its range, which is passed through" },
};

export const RANDOM_DRAWS: Record<string, string> = Object.fromEntries(
  [
    "unison_0.2_spread_0.4_detune_0", "unison_0.5_spread_0.4_detune_0", "unison_0.5_spread_0_detune_0.4",
    "unison_1_spread_0.4_detune_0.4", "unison_1_spread_0_detune_0",
  ].map((n) => [n, "unison layers start at random phases"]).concat([["global-groove", "every sound is a 2 ms noise burst"]]),
);
export const SEEDS = 8;

export function passes(r: Row): boolean {
  if (r.error || !r.report) return false;
  const x = r.report;
  return x.pass;
}

export interface Retirement {
  ready: boolean;
  blocking: string[];
}

/** The gate of the spec: every device and every reference song passes, and the user signed off the A/B listen. */
export function retirement(run: Run): Retirement {
  const blocking = run.rows.filter((r) => !passes(r)).map((r) => `${r.level} ${r.name}${r.error ? ` (${r.error})` : ""}`);
  if (run.skipped) blocking.push(`reference songs not run: ${run.skipped}`);
  if (!run.rows.some((r) => r.level === "device")) blocking.push("no device-level rows");
  if (run.signoff.decision !== "approved") blocking.push(`A/B listening sign-off ${run.signoff.decision}`);
  return { ready: blocking.length === 0, blocking };
}

/** Real-time factor of the unified render (audio seconds per render second). */
export function realTime(r: Row): number | undefined {
  return r.secs && r.ourTime ? r.secs / r.ourTime : undefined;
}

// ---------- report ----------

const f2 = (x: number) => (Number.isFinite(x) ? x.toFixed(2) : String(x));

function table(rows: Row[]): string[] {
  const out = [
    "| item | engine | loudness Δ dB | worst band Δ dB | onset lag (frames) | onset corr | peak onsets matched | result |",
    "|---|---|---:|---:|---:|---:|---:|---|",
  ];
  for (const r of rows) {
    if (!r.report) {
      out.push(`| ${r.name} | ${r.engine} | | | | | | FAIL: ${r.error ?? "no result"} |`);
      continue;
    }
    const x = r.report;
    const worst = x.bands.filter((b) => b.pass || Math.abs(b.diff) > 0).reduce((m, b) => (Math.abs(b.diff) > Math.abs(m) ? b.diff : m), 0);
    const mark = passes(r) ? (r.ensemble ? "pass (seed mean)" : "pass") : "**FAIL**";
    out.push(`| ${r.name} | ${r.engine} | ${f2(x.loudness.diff)} | ${f2(worst)} | ${x.onsets.lag} | ${f2(x.onsets.corr ?? NaN)} | ${x.onsets.matched}/${x.onsets.total} (+${x.onsets.extra}) | ${mark} |`);
  }
  return out;
}

export function formatRun(run: Run): string {
  const dev = run.rows.filter((r) => r.level === "device");
  const songs = run.rows.filter((r) => r.level === "song");
  const gate = retirement(run);
  const L: string[] = [
    "# Unified synth parity report",
    "",
    "Metrics only, no audio. Regenerate with `bun tools/unisynth-parity/parity.ts` (design D11).",
    "",
    `Generated ${run.generated}. Tolerances: loudness ±${DEFAULT_TOL.loudnessDb} dB (BS.1770-4), ±${DEFAULT_TOL.bandDb} dB per octave band 63 Hz–16 kHz (bands more than ${BAND_FLOOR_DB} dB below the loudest reference band are not compared), onsets within ${ONSET_FRAMES.ws} frames (WaveSabre) or ${ONSET_FRAMES.tf} frames (Tunefish), timing by the cross-correlation of the onset-strength curves: lag within that window and correlation ≥ ${MIN_ALIGNMENT}. (Peak-picked onsets are listed too, but not judged: for two different engines the picker keeps a different one of two neighbouring hits in each render.)`,
    "",
    `## Retirement: ${gate.ready ? "READY" : "BLOCKED"}`,
    "",
    ...(gate.ready ? ["Every device and reference song passes and the A/B listening is signed off."] : ["Blocked by:", ...gate.blocking.map((b) => `- ${b}`)]),
    "",
    "## Device level",
    "",
    `Every WaveSabre test song (\`crates/wavesabre/tests/songs.rs\`) and every Tunefish synthetic patch (\`crates/tfsynth/tests/patches.rs\`): ${dev.filter(passes).length} of ${dev.length} pass.`,
    "",
    ...table(dev),
    "",
    "## Reference songs",
    "",
  ];
  if (run.skipped) L.push(`Skipped: ${run.skipped}`);
  else L.push(`${songs.filter(passes).length} of ${songs.length} pass.`, "", ...table(songs));
  const ens = run.rows.filter((r) => r.ensemble);
  if (ens.length) {
    L.push("", "## Judged on several random seeds", "", `The reference is one random draw, so these were rendered with ${SEEDS} seeds; the mean of ours must agree within the usual tolerance plus twice the spread between seeds (loudness ${DEFAULT_TOL.loudnessDb} dB, bands ${DEFAULT_TOL.bandDb} dB):`, "");
    for (const r of ens) L.push(`- \`${r.name}\` (${r.ensemble!.why}): loudness sd ${f2(r.ensemble!.loudnessSd)} dB, band sd ${r.ensemble!.bandSd.map(f2).join(" / ")} dB (63 Hz … 16 kHz)`);
  }
  const acc = run.rows.filter((r) => ACCEPTED[r.name]);
  if (acc.length)
    L.push("", "## Accepted differences", "", "These are judged against a wider tolerance on purpose, because the unified synth does not run a filter that is set open:", "", ...acc.map((r) => `- \`${r.name}\`: loudness ±${ACCEPTED[r.name].loudnessDb} dB, bands ±${ACCEPTED[r.name].bandDb} dB (${ACCEPTED[r.name].why})`));
  const warned = run.rows.filter((r) => r.warnings?.length);
  if (warned.length) L.push("", "## Converter warnings", "", ...warned.flatMap((r) => r.warnings!.map((w) => `- \`${r.name}\`: ${w}`)));
  const timed = songs.filter((r) => r.secs && r.ourTime);
  if (timed.length) {
    L.push(
      "", "## Speed (one thread)", "",
      `Rendering the song to 16-bit PCM on this machine, legacy engine and converted song alike, with the speed build (\`[profile.fast]\`: the release profile at opt-level 3). The floor is the spec's bar for the unified synth: Elysian ≥ ${SPEED_FLOOR.elysian}×, no_remorse ≥ ${SPEED_FLOOR["no-remorse"]}× real time.`,
      "", "| song | audio s | legacy render s | legacy × real time | unified render s | unified × real time | floor |", "|---|---:|---:|---:|---:|---:|---|",
    );
    for (const r of timed) {
      const x = realTime(r)!, floor = SPEED_FLOOR[r.name];
      L.push(`| ${r.name} | ${r.secs!.toFixed(1)} | ${r.legacyTime?.toFixed(3) ?? ""} | ${r.legacyTime ? (r.secs! / r.legacyTime).toFixed(1) : ""} | ${r.ourTime!.toFixed(3)} | ${x.toFixed(1)} | ${floor ? `≥ ${floor}× ${x >= floor ? "ok" : "**FAIL**"}` : ""} |`);
    }
    const sb = songs.filter((r) => r.sizeBuild);
    if (sb.length) {
      L.push("", "On the profile the player ships with (`release`, opt-level z, built for size) the same songs run slower, the unified synth more so than the legacy engines (its code is more generic and leans on the optimiser to inline it):", "", "| song | legacy × real time | unified × real time |", "|---|---:|---:|");
      for (const r of sb) L.push(`| ${r.name} | ${(r.secs! / r.sizeBuild!.legacyTime).toFixed(1)} | ${(r.secs! / r.sizeBuild!.ourTime).toFixed(1)} |`);
    }
  }
  if (run.sizes) L.push("", "## Size", "", run.sizes.trim());
  L.push("", "## A/B listening", "", run.signoff.decision === "pending" ? "Pending: the six reference songs are rendered both ways for the user to compare." : `Decision: **${run.signoff.decision}**${run.signoff.by ? ` by ${run.signoff.by}` : ""}${run.signoff.date ? ` on ${run.signoff.date}` : ""}.`);
  if (run.signoff.notes) L.push("", run.signoff.notes.trim());
  return L.join("\n") + "\n";
}

// ---------- inputs ----------

export interface Songs {
  songs: { name: string; engine: Engine; file: string; bank?: string }[];
  skipped?: string;
}

/** The reference songs present on this machine, from the demo-songs manifest; `skipped` says why none ran. */
export function referenceSongs(env: Record<string, string | undefined>, manifest = join(ROOT, "tools/demo-songs/songs.json")): Songs {
  const m = JSON.parse(readFileSync(manifest, "utf8")) as { songs: { name: string; source: string; file: string }[] };
  const found: Songs["songs"] = [];
  const missing: string[] = [];
  for (const e of m.songs) {
    const engine: Engine = e.source === "wavesabre" ? "ws" : "tf";
    const dir = env[engine === "ws" ? "WS_SONG_DIR" : "TF_SONGS"];
    const file = dir && join(dir, e.file);
    if (file && existsSync(file)) {
      const bank = env.GM_DLS ?? (dir && existsSync(join(dir, "gm.dls")) ? join(dir, "gm.dls") : existsSync(MACOS_BANK) ? MACOS_BANK : undefined);
      found.push({ name: e.name, engine, file, bank: engine === "ws" ? bank : undefined });
    } else missing.push(e.name);
  }
  if (!found.length) return { songs: [], skipped: `the synth lab songs are absent (set WS_SONG_DIR and TF_SONGS); ${missing.join(", ")} not run` };
  return { songs: found, skipped: missing.length ? `${missing.join(", ")} not found in WS_SONG_DIR / TF_SONGS` : undefined };
}

// ---------- the pipeline ----------

function sh(cmd: string[], env?: Record<string, string>): { ok: boolean; out: string; err: string } {
  const p = Bun.spawnSync(cmd, { cwd: ROOT, env: { ...process.env, ...env }, stdout: "pipe", stderr: "pipe" });
  return { ok: p.exitCode === 0, out: p.stdout.toString(), err: p.stderr.toString().trim() };
}

/** The speed build: the release profile with opt-level 3 (`[profile.fast]`), so legacy and unified are timed alike. */
export const PROFILE = "fast";
const bin = (n: string) => join(ROOT, "target", PROFILE, n);
const ex = (n: string) => join(ROOT, "target", PROFILE, "examples", n);

function build(): void {
  const b = sh(["cargo", "build", "-q", "--profile", PROFILE, "-p", "synth-convert", "--bin", "synth-convert", "--example", "legacy"]);
  if (!b.ok) throw new Error(`cargo build failed:\n${b.err}`);
}

/** Times the legacy render and the unified render of `conv` on `profile` (a throwaway output file). */
function timeOn(profile: string, file: string, conv: string, tmp: string, tfm: boolean, bank?: string): { legacyTime: number; ourTime: number } | undefined {
  const b = sh(["cargo", "build", "-q", "--profile", profile, "-p", "synth-convert", "--example", "legacy"]);
  if (!b.ok) return undefined;
  const l = sh([join(ROOT, "target", profile === "release" ? "release" : profile, "examples/legacy"), file, tmp, ...(tfm ? ["4"] : bank ? [bank] : [])]);
  const u = sh(["cargo", "run", "-q", "--profile", profile, "-p", "unisynth", "--features", "std", "--example", "render", "--", conv, tmp, ...(bank ? [bank] : [])]);
  const ourTime = Number(/rendered in ([\d.]+) s/.exec(u.err)?.[1]);
  return l.ok && u.ok && ourTime ? { legacyTime: Number(l.out.trim().split(" ")[2]), ourTime } : undefined;
}

/** Legacy render → convert → unified render → compare, for one legacy file. */
export function runItem(level: Level, name: string, engine: Engine, file: string, work: string, bank?: string, reuse = false): Row {
  const row: Row = { level, name, engine };
  const ref = join(work, `${name}_legacy.wav`), conv = join(work, `${name}.unified.bin`), ours = join(work, `${name}_unified.wav`);
  const cache = join(work, `${name}.json`);
  const tfm = file.endsWith(".tfm");
  if (reuse && existsSync(cache) && existsSync(ref) && existsSync(ours)) Object.assign(row, JSON.parse(readFileSync(cache, "utf8")));
  else {
    const r = render(row, file, tfm, ref, conv, ours, bank);
    if (r) return r;
    if (SPEED_FLOOR[name]) row.sizeBuild = timeOn("release", file, conv, ours.replace(/\.wav$/, "_size.wav"), tfm, bank);
    writeFileSync(cache, JSON.stringify({ legacyTime: row.legacyTime, ourTime: row.ourTime, warnings: row.warnings, sizeBuild: row.sizeBuild }));
  }
  return judge(row, ref, ours);
}

/** The three renders of one item; returns a failed row when a step fails. */
function render(row: Row, file: string, tfm: boolean, ref: string, conv: string, ours: string, bank?: string): Row | undefined {
  const legacy = sh([ex("legacy"), file, ref, ...(tfm ? ["4"] : bank ? [bank] : [])]);
  if (!legacy.ok) return { ...row, error: `legacy render: ${legacy.err.split("\n").pop()}` };
  row.legacyTime = Number(legacy.out.trim().split(" ")[2]);
  const c = sh([bin("synth-convert"), tfm ? "tfm" : "ws", file, conv]);
  if (!c.ok) return { ...row, error: `convert: ${c.err.split("\n").pop()}` };
  row.warnings = c.err.split("\n").filter((l) => /warning/i.test(l)).map((l) => l.replace(/^.*?warning:?\s*/i, ""));
  const u = sh(["cargo", "run", "-q", "--profile", PROFILE, "-p", "unisynth", "--features", "std", "--example", "render", "--", conv, ours, ...(bank ? [bank] : [])]);
  if (!u.ok) return { ...row, error: `unified render: ${u.err.slice(-300)}` };
  row.ourTime = Number(/rendered in ([\d.]+) s/.exec(u.err)?.[1]);
  if (RANDOM_DRAWS[row.name])
    for (let k = 2; k <= SEEDS; k++) {
      const x = sh(["cargo", "run", "-q", "--profile", PROFILE, "-p", "unisynth", "--features", "std", "--example", "render", "--", conv, ours.replace(/\.wav$/, `_s${k}.wav`), ...(bank ? [bank] : [])], { UNISYNTH_SEED: String(k) });
      if (!x.ok) return { ...row, error: `unified render (seed ${k}): ${x.err.slice(-300)}` };
    }
}

function judge(row: Row, ref: string, ours: string): Row {
  const { name, engine } = row;
  try {
    const a = readWav(ours), r = readWav(ref);
    row.secs = a.ch[0].length / a.rate;
    const tol: Tolerance = { ...DEFAULT_TOL, onsetFrames: ONSET_FRAMES[engine], bandFloorDb: BAND_FLOOR_DB, onsetAlignment: MIN_ALIGNMENT, ...ACCEPTED[name] };
    row.report = compare(a, r, tol);
    if (RANDOM_DRAWS[name]) row.report = judgeSeeds(row, ref, ours, row.report, tol);
  } catch (e) {
    return { ...row, error: String((e as Error).message) };
  }
  return row;
}

const mean = (x: number[]) => x.reduce((p, v) => p + v, 0) / x.length;
const sd = (x: number[]) => Math.sqrt(mean(x.map((v) => (v - mean(x)) ** 2)));

/** Loudness and octave bands of `SEEDS` renders of ours against the reference; onsets stay those of seed 1. */
function judgeSeeds(row: Row, ref: string, ours: string, first: Report, tol: Tolerance): Report {
  const r = readWav(ref);
  const runs = Array.from({ length: SEEDS }, (_, k) => readWav(k === 0 ? ours : ours.replace(/\.wav$/, `_s${k + 1}.wav`)));
  const loud = runs.map(loudness), bands = runs.map(octaveLtas), rl = loudness(r), rb = octaveLtas(r);
  const top = Math.max(...rb.map((b) => b.db));
  const loudnessSd = sd(loud);
  const bandSd = rb.map((_, i) => sd(bands.map((b) => b[i].db)));
  row.ensemble = { seeds: SEEDS, loudnessSd, bandSd, why: RANDOM_DRAWS[row.name] };
  const l = { ours: mean(loud), ref: rl, diff: mean(loud) - rl, pass: Math.abs(mean(loud) - rl) <= tol.loudnessDb + 2 * loudnessSd };
  const bs = rb.map((b, i) => {
    const ours = mean(bands.map((x) => x[i].db)), diff = ours - b.db;
    return { fc: b.fc, ours, ref: b.db, diff, pass: b.db < top - (tol.bandFloorDb ?? 0) || Math.abs(diff) <= tol.bandDb + 2 * bandSd[i] };
  });
  return { loudness: l, bands: bs, onsets: first.onsets, pass: l.pass && bs.every((b) => b.pass) && first.onsets.pass };
}

function deviceFiles(work: string, bank?: string): { name: string; engine: Engine; file: string; bank?: string }[] {
  const ws = join(work, "ws-devices"), tf = join(work, "tf-devices");
  mkdirSync(ws, { recursive: true });
  mkdirSync(tf, { recursive: true });
  const w = sh(["cargo", "run", "-q", "--profile", PROFILE, "-p", "wavesabre", "--example", "render_device", "--", ws, ...(bank ? [bank] : [])]);
  if (!w.ok) throw new Error(`wavesabre render_device: ${w.err}`);
  const d = sh(["cargo", "test", "-q", "--profile", PROFILE, "-p", "synth-convert", "--test", "tunefish", "dump_the_synthetic_patch_songs"], { PARITY_DUMP: tf });
  if (!d.ok) throw new Error(`tunefish patch dump: ${d.err}`);
  const list = (dir: string, ext: string, engine: Engine) =>
    readdirSync(dir).filter((f) => f.endsWith(ext)).sort().map((f) => ({ name: f.slice(0, -ext.length), engine, file: join(dir, f), bank: engine === "ws" ? bank : undefined }));
  return [...list(ws, ".bin", "ws"), ...list(tf, ".song", "tf")];
}

export function readSignoff(path: string): Signoff {
  return existsSync(path) ? (JSON.parse(readFileSync(path, "utf8")) as Signoff) : { decision: "pending" };
}

function main() {
  const a = process.argv.slice(2);
  const reportPath = a.includes("--report") ? a[a.indexOf("--report") + 1] : join(import.meta.dir, "report.md");
  const noSongs = a.includes("--no-songs");
  // --reuse <dir>: keep the renders of an earlier run there and only compare again (the cache holds the timings too)
  const reuseDir = a.includes("--reuse") ? a[a.indexOf("--reuse") + 1] : undefined;
  const work = reuseDir ?? join(process.env.TMPDIR ?? "/tmp", `unisynth-parity-${process.pid}`);
  mkdirSync(work, { recursive: true });
  build();
  const bank = process.env.GM_DLS ?? (existsSync(MACOS_BANK) ? MACOS_BANK : undefined);
  const rows: Row[] = [];
  for (const d of deviceFiles(work, bank)) rows.push(runItem("device", d.name, d.engine, d.file, work, d.bank, !!reuseDir));
  let skipped: string | undefined;
  if (noSongs) skipped = "--no-songs";
  else {
    const s = referenceSongs(process.env);
    skipped = s.skipped;
    if (!s.songs.length) console.log(`skipped: ${s.skipped}`);
    for (const d of s.songs) rows.push(runItem("song", d.name, d.engine, d.file, work, d.bank, !!reuseDir));
  }
  const sizesFile = join(import.meta.dir, "size.md");
  const run: Run = {
    rows,
    skipped,
    signoff: readSignoff(join(import.meta.dir, "signoff.json")),
    sizes: existsSync(sizesFile) ? readFileSync(sizesFile, "utf8") : undefined,
    generated: new Date().toISOString().slice(0, 10),
  };
  writeFileSync(reportPath, formatRun(run));
  for (const r of rows) console.log(`${passes(r) ? "ok  " : "FAIL"} ${r.level} ${r.name}${r.error ? `: ${r.error}` : ""}`);
  const failing = rows.filter((r) => !passes(r));
  console.log(`${rows.length - failing.length}/${rows.length} pass; report: ${reportPath}`);
  process.exit(failing.length ? 1 : 0);
}

if (import.meta.main) main();
