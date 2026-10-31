// Mini-cauldron parity (change add-cauldron-xm-import): the three TBL songs (Jizz, Stash song 1, Stash BLUISH) and
// every generated phrase, converted by `synth-convert xm` / `xm-phrase` and rendered by `unisynth`, against the
// bit-exact reference: `openmpt123` renders of the XMs rebuilt from `research/tbl-mini-cauldron/ref/mcauldron*.py`
// (`out/listen/ref44/<song>.wav`) and each decimated phrase at the full internal rate, before the decimation, cut at
// its stored Nyquist and resampled to 44.1 kHz (`out/listen/ref44bw/<phrase>.wav`, `tools/fullrate_phrases.py`): the
// port drops the decimation by design, and the stored phrases' upper bands carry its folded aliasing (up to +7 dB).
// Song metrics as the other suites (1 dB loudness, 3 dB per octave band 63 Hz–16 kHz with the band floor, onset-strength
// alignment); phrases judge only the octave bands lying wholly below the phrase's stored Nyquist (past it, a band's level
// is the playback filters', not the synth's).
// Writes `cauldron-report.md` (metrics only); audio and converted songs stay in the git-ignored `out/`.
//
//   bun tools/unisynth-parity/cauldron.ts [--report path] [--songs|--phrases] [--self] [name…]
//   bun tools/unisynth-parity/cauldron.ts --onsets   (the XM replay's trigger times against openmpt123 playing the
//     XM with every sample a click: ≥ 99 % of our clicks within 100 frames)
//     --self   compares every reference with itself (harness check: all differences 0)
//     CAULDRON_OUT overrides research/tbl-mini-cauldron/out
// Exit 0 when everything that ran passed (or the material is absent: skip), 1 otherwise.
import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";
import { compare, DEFAULT_TOL, readWav, type Audio, type Report } from "../synth-parity/metrics.ts";
import { BAND_FLOOR_DB, MIN_ALIGNMENT, PROFILE } from "./parity.ts";
import { cut } from "./v2.ts";

const ROOT = resolve(import.meta.dir, "../..");
const RESEARCH = join(ROOT, "research/tbl-mini-cauldron");
export const OUT = process.env.CAULDRON_OUT ?? join(RESEARCH, "out");

/** Song → (rebuilt XM, patch data). */
export const SONGS: Record<string, { xm: string; patches: string }> = {
  jizz: { xm: "jizz.xm", patches: "patches_jizz.json" },
  stash_song1: { xm: "stash_song1.xm", patches: "patches_stash.json" },
  stash_bluish: { xm: "stash_bluish.xm", patches: "patches_stash.json" },
};

/** One entry of `out/listen/phrases.json` (written by `tools/rebuild_xm.py`). */
export interface Phrase {
  name: string;
  song: string; // jizz | stash_bluish | stash_song0
  record: number;
  variant: number;
  decim: number;
  len: number;
  rate_hz: number;
  nyquist_hz: number;
}

export interface Row {
  name: string;
  kind: "song" | "phrase";
  error?: string;
  report?: Report;
  /** Seconds played through ping-pong loops (converter's `pingpong-seconds` line), songs only. */
  pingpong?: number;
  warnings?: string[];
}

export const passes = (r: Row) => !r.error && !!r.report?.pass;

const SONG_TOL = { ...DEFAULT_TOL, bandFloorDb: BAND_FLOOR_DB, onsetAlignment: MIN_ALIGNMENT };

export function judgeSong(ours: Audio, ref: Audio): Report {
  const [a, b] = cut(ours, ref);
  return compare(a, b, SONG_TOL);
}

/** Our mid channel `(L + R)/2` when the reference is mono (the phrase references are the stored mono samples). */
const asRef = (ours: Audio, ref: Audio): Audio =>
  ref.ch.length === 1 && ours.ch.length === 2 ? { rate: ours.rate, ch: [ours.ch[0].map((x, i) => (x + ours.ch[1][i]) / 2)] } : ours;

/** A phrase: as a song, but octave bands reaching above `nyquist` are not judged (marked pass, kept for reading). */
export function judgePhrase(ours: Audio, ref: Audio, nyquist: number): Report {
  const [a, b] = cut(asRef(ours, ref), ref);
  const r = compare(a, b, SONG_TOL);
  // only bands lying wholly below the stored Nyquist: above it (or straddling it) a band's level is the playback
  // and anti-alias filters' (ours, openmpt's, the reference's brick wall), not the synth's
  const bands = r.bands.map((x) => (x.fc * Math.SQRT2 > nyquist ? { ...x, pass: true } : x));
  return { ...r, bands, pass: r.loudness.pass && r.onsets.pass && bands.every((x) => x.pass) };
}

/** `lead-frames <n>` from `xm-phrase`'s stderr (where the phrase output starts), 0 when absent. */
export const leadFrames = (stderr: string) => Number(/lead-frames (\d+)/.exec(stderr)?.[1] ?? 0);

/** `a` without its first `n` frames. */
export const dropLead = (a: Audio, n: number): Audio => ({ rate: a.rate, ch: a.ch.map((c) => c.subarray(n)) });

/** `pingpong-seconds <x>` from the converter's stderr, 0 when absent. */
export const pingpongSeconds = (stderr: string) => Number(/pingpong-seconds ([\d.]+)/.exec(stderr)?.[1] ?? 0);

const f2 = (x: number) => (Number.isFinite(x) ? x.toFixed(2) : String(x));

export function formatRun(rows: Row[], generated: string, signoff: { decision: string; by?: string; date?: string; notes?: string }, skipped?: string): string {
  const L = [
    "# Mini-cauldron parity (change add-cauldron-xm-import)",
    "",
    `Generated ${generated} by \`bun tools/unisynth-parity/cauldron.ts\`. Reference: \`openmpt123\` (44.1 kHz) of the XMs rebuilt from the bit-exact Python references, and each phrase at the full internal rate cut at its stored Nyquist (the port drops the decimation, so its aliasing is not judged), resampled to 44.1 kHz; ours: \`synth-convert xm\` / \`xm-phrase\` → \`unisynth\` (\`--profile ${PROFILE}\`). Loudness within 1 dB, octave bands within 3 dB (bands ${BAND_FLOOR_DB} dB under the loudest skipped; phrases judge only bands lying wholly below their stored Nyquist), onset-strength alignment (lag within 100 frames, correlation ≥ ${MIN_ALIGNMENT}). Audio stays in \`research/tbl-mini-cauldron/out\`.`,
    "",
  ];
  if (skipped) L.push(`Skipped: ${skipped}.`, "");
  const section = (title: string, rs: Row[], songs: boolean) => {
    if (!rs.length) return;
    L.push(`## ${title}`, "", `${rs.filter(passes).length} of ${rs.length} pass.`, "", `| name | result | loudness Δ dB | worst band Δ dB | alignment corr |${songs ? " ping-pong s |" : ""} notes |`, `|---|---|---:|---:|---:|${songs ? "---:|" : ""}---|`);
    for (const r of rs) {
      if (r.error || !r.report) {
        L.push(`| ${r.name} | ERROR | | | |${songs ? " |" : ""} ${r.error ?? "no report"} |`);
        continue;
      }
      const judged = r.report.bands.filter((b) => Number.isFinite(b.diff));
      const worst = judged.reduce((m, b) => (Math.abs(b.diff) > Math.abs(m.diff) ? b : m), judged[0] ?? r.report.bands[0]);
      L.push(`| ${r.name} | ${r.report.pass ? "PASS" : "FAIL"} | ${f2(r.report.loudness.diff)} | ${f2(worst.diff)} @ ${worst.fc} | ${r.report.onsets.corr === undefined ? "" : f2(r.report.onsets.corr)} |${songs ? ` ${f2(r.pingpong ?? 0)} |` : ""} ${(r.warnings ?? []).length ? `${r.warnings!.length} converter warning(s)` : ""} |`);
    }
    L.push("");
  };
  section("Songs", rows.filter((r) => r.kind === "song"), true);
  section("Phrases", rows.filter((r) => r.kind === "phrase"), false);
  L.push("## A/B listening", "", signoff.decision === "pending" ? "Pending: the user compares `out/listen/*.mp3` (reference) with `out/songs/*.mp3` (converted)." : `Decision: **${signoff.decision}**${signoff.by ? ` by ${signoff.by}` : ""}${signoff.date ? ` on ${signoff.date}` : ""}.`);
  if (signoff.notes) L.push("", signoff.notes);
  return L.join("\n") + "\n";
}

function sh(cmd: string[]): { ok: boolean; out: string; err: string } {
  const p = Bun.spawnSync(cmd, { cwd: ROOT, stdout: "pipe", stderr: "pipe" });
  return { ok: p.exitCode === 0, out: p.stdout.toString(), err: p.stderr.toString().trim() };
}

const bin = (n: string) => join(ROOT, "target", PROFILE, n);
const ex = (n: string) => join(ROOT, "target", PROFILE, "examples", n);
const ref44 = (name: string) => join(OUT, "listen/ref44", `${name}.wav`);
const ref44bw = (name: string) => join(OUT, "listen/ref44bw", `${name}.wav`);
/** A decimated phrase's reference is the alias-free one; an undecimated phrase has no aliasing to remove (and ref44's
 * resampler rolls off near its 17.7 kHz Nyquist as ours does). */
export const phraseRef = (p: { name: string; decim: number }) => (p.decim > 1 ? ref44bw(p.name) : ref44(p.name));

/** The song conversion the parity judges: the XM's layers and pitch tracks kept (change compact-cauldron-xm-import). */
export const PARITY_OPTIONS = ["--keep-layers", "--split-pitches"];

/** Convert (`args` after the subcommand, `post` after the output) → render → read; `self` reads the reference (`ref`) instead. */
function ours(sub: string[], name: string, work: string, self: boolean, ref = ref44(name), post: string[] = []): { audio?: Audio; err?: string; stderr?: string } {
  if (self) return { audio: readWav(ref) };
  const conv = join(work, `${name}.song`), wav = join(work, `${name}.wav`);
  const c = sh([bin("synth-convert"), ...sub, conv, ...post]);
  if (!c.ok) return { err: `convert: ${c.err.split("\n").pop()}` };
  const u = sh([ex("render"), conv, wav]);
  if (!u.ok) return { err: `render: ${u.err.slice(-300)}` };
  return { audio: readWav(wav), stderr: c.err };
}

export function runSong(name: string, work: string, self = false): Row {
  const row: Row = { name, kind: "song" };
  try {
    const s = SONGS[name];
    const o = ours(["xm", join(OUT, "listen", s.xm), join(RESEARCH, "ref", s.patches)], name, work, self, ref44(name), PARITY_OPTIONS);
    if (o.err) return { ...row, error: o.err };
    row.warnings = (o.stderr ?? "").split("\n").filter((l) => /warning/i.test(l));
    row.pingpong = pingpongSeconds(o.stderr ?? "");
    row.report = judgeSong(dropLead(o.audio!, leadFrames(o.stderr ?? "")), readWav(ref44(name)));
  } catch (e) {
    return { ...row, error: String((e as Error).message) };
  }
  return row;
}

export function runPhrase(p: Phrase, work: string, self = false): Row {
  const row: Row = { name: p.name, kind: "phrase" };
  try {
    const patches = join(RESEARCH, "ref", p.song === "jizz" ? "patches_jizz.json" : "patches_stash.json");
    const call = p.song.replace(/^stash_/, "");
    const ref = phraseRef(p);
    const o = ours(["xm-phrase", patches, call, String(p.record), String(p.variant)], p.name, work, self, ref);
    if (o.err) return { ...row, error: o.err };
    row.report = judgePhrase(dropLead(o.audio!, leadFrames(o.stderr ?? "")), readWav(ref), p.nyquist_hz);
  } catch (e) {
    return { ...row, error: String((e as Error).message) };
  }
  return row;
}

// ---------- XM replay timing (task 5.2): the replay's triggers as clicks against the reference songs ----------

/** `synth-convert xm-triggers` output: "<start s> <volume> <offset>" per line. */
export const parseTriggers = (out: string) =>
  out.trim().split("\n").filter(Boolean).map((l) => {
    const [at, vol, offset] = l.split(/\s+/).map(Number);
    return { at, vol, offset: offset ?? 0 };
  });

/** The XM with every sample's data replaced by a 32-sample click at its start (lengths, loops and offsets kept), for
 * rendering the reference's timing with openmpt123: the "silent placeholder instruments plus click markers". */
export function clickXm(b: Uint8Array): Uint8Array {
  const x = new Uint8Array(b), v = new DataView(x.buffer);
  const u16 = (p: number) => v.getUint16(p, true), u32 = (p: number) => v.getUint32(p, true);
  let p = 60 + u32(60);
  const [npat, nins] = [u16(70), u16(72)];
  for (let i = 0; i < npat; i++) p += u32(p) + u16(p + 7);
  for (let i = 0; i < nins; i++) {
    const size = u32(p), ns = u16(p + 27);
    if (!ns) {
      p += size;
      continue;
    }
    let q = p + size;
    const heads = Array.from({ length: ns }, (_, j) => ({ len: u32(q + 40 * j), bits16: (x[q + 40 * j + 14] & 16) !== 0 }));
    q += 40 * ns;
    for (const h of heads) {
      const w = h.bits16 ? 2 : 1, n = h.len / w;
      let prev = 0;
      for (let k = 0; k < n; k++) {
        const val = k < 32 ? Math.round((k % 2 ? -1 : 1) * Math.exp(-k / 8) * (h.bits16 ? 30000 : 120)) : 0;
        const d = val - prev;
        prev = val;
        if (h.bits16) v.setInt16(q + 2 * k, ((d + 32768) & 0xffff) - 32768, true);
        else x[q + k] = d & 0xff;
      }
      q += h.len;
    }
    p = q;
  }
  return x;
}

/** `clickXm`'s click at each audible trigger that starts within it (a sample offset past 32 starts in silence),
 * scaled by its volume (mono). */
export function clickTrack(t: { at: number; vol: number; offset?: number }[], seconds: number, rate: number): Audio {
  const x = new Float32Array(Math.round(seconds * rate));
  for (const { at, vol, offset } of t) {
    if (vol <= 0 || (offset ?? 0) >= 32) continue;
    const s = Math.round(at * rate);
    for (let i = 0; i < 32 && s + i < x.length; i++) x[s + i] += vol * (i % 2 ? -1 : 1) * Math.exp(-i / 8);
  }
  return { rate, ch: [x] };
}

/** Click starts in a render of clicks on silence: the first sample above `eps` of the peak after ≥ `gap` quiet ones. */
export function clickStarts(a: Audio, gap = 40, eps = 1e-3): number[] {
  const x = a.ch[0];
  const peak = x.reduce((p, v) => Math.max(p, Math.abs(v)), 0) || 1;
  const out: number[] = [];
  let quiet = gap;
  for (let i = 0; i < x.length; i++) {
    if (Math.abs(x[i]) > eps * peak) {
      if (quiet >= gap) out.push(i);
      quiet = 0;
    } else quiet++;
  }
  return out;
}

/** The share of `ours` click starts with a `ref` click start within `tol` frames, and the median offset (frames,
 * positive = ours later) of the matched ones. One-sided: the reference may click more (a loop replaying the click). */
export function onsetMatch(ours: Audio, ref: Audio, tol: number): { matched: number; offset: number; oursCount: number; refCount: number } {
  const [o, r] = [clickStarts(ours), clickStarts(ref)];
  let j = 0;
  const d: number[] = [];
  for (const t of o) {
    while (j + 1 < r.length && Math.abs(r[j + 1] - t) <= Math.abs(r[j] - t)) j++;
    if (r.length && Math.abs(r[j] - t) <= tol) d.push(t - r[j]);
  }
  d.sort((a, b) => a - b);
  return { matched: o.length ? d.length / o.length : 0, offset: d.length ? d[d.length >> 1] : 0, oursCount: o.length, refCount: r.length };
}

/** Clicks of ours that must land within the onset tolerance of openmpt's. */
export const MIN_CLICK_MATCH = 0.99;

function runOnsets(name: string, work: string): boolean {
  const xm = join(OUT, "listen", SONGS[name].xm);
  const t = sh([bin("synth-convert"), "xm-triggers", xm]);
  if (!t.ok) return (console.log(`onsets ${name}: ERROR ${t.err.split("\n").pop()}`), false);
  // the reference's timing: openmpt123 playing the XM with every sample a click
  const clicks = join(work, `${name}.clicks.xm`);
  writeFileSync(clicks, clickXm(readFileSync(xm)));
  const o = sh(["openmpt123", "--quiet", "--batch", "--render", "--force", "--samplerate", "44100", clicks]);
  if (!o.ok) return (console.log(`onsets ${name}: openmpt123 failed ${o.err.slice(-200)}`), false);
  const ref = readWav(`${clicks}.wav`);
  const m = onsetMatch(clickTrack(parseTriggers(t.out), ref.ch[0].length / ref.rate, ref.rate), ref, DEFAULT_TOL.onsetFrames);
  const ok = m.matched >= MIN_CLICK_MATCH;
  console.log(`onsets ${name}: ${ok ? "PASS" : "FAIL"} ${(100 * m.matched).toFixed(1)} % of our ${m.oursCount} clicks within ${DEFAULT_TOL.onsetFrames} frames of openmpt's (${m.refCount}), median offset ${m.offset} frames`);
  return ok;
}

function main() {
  const a = process.argv.slice(2);
  const flag = (k: string) => a.includes(k) && a.splice(a.indexOf(k), 1).length > 0;
  const opt = (k: string) => (a.includes(k) ? a.splice(a.indexOf(k), 2)[1] : undefined);
  const reportPath = opt("--report") ?? join(import.meta.dir, "cauldron-report.md");
  const onlySongs = flag("--songs"), onlyPhrases = flag("--phrases"), self = flag("--self"), onsetsOnly = flag("--onsets");
  const signoffPath = join(import.meta.dir, "cauldron-signoff.json");
  const signoff = existsSync(signoffPath) ? JSON.parse(readFileSync(signoffPath, "utf8")) : { decision: "pending" };
  const listen = join(OUT, "listen");
  if (!existsSync(join(listen, "phrases.json")) || !existsSync(join(listen, "ref44")) || !existsSync(join(listen, "ref44bw"))) {
    console.log(`skip: mini-cauldron reference material absent (${listen}; run research/tbl-mini-cauldron/tools/run_all.sh)`);
    process.exit(0);
  }
  if (!self) {
    const b = sh(["cargo", "build", "-q", "--profile", PROFILE, "-p", "synth-convert", "--bin", "synth-convert"]);
    const e = sh(["cargo", "build", "-q", "--profile", PROFILE, "-p", "unisynth", "--features", "std", "--example", "render"]);
    if (!b.ok || !e.ok) throw new Error(`cargo build failed:\n${b.err}\n${e.err}`);
  }
  const work = join(OUT, "parity");
  if (onsetsOnly) {
    mkdirSync(work, { recursive: true });
    const ok = Object.keys(SONGS).map((n) => runOnsets(n, work));
    process.exit(ok.every(Boolean) ? 0 : 1);
  }
  mkdirSync(work, { recursive: true });
  const phrases: Phrase[] = JSON.parse(readFileSync(join(listen, "phrases.json"), "utf8"));
  const pick = (n: string) => !a.length || a.includes(n);
  const rows: Row[] = [];
  if (!onlyPhrases) for (const n of Object.keys(SONGS).filter(pick)) rows.push(runSong(n, work, self));
  if (!onlySongs) for (const p of phrases.filter((p) => pick(p.name) || pick(p.song))) rows.push(runPhrase(p, work, self));
  for (const r of rows) console.log(`${r.kind} ${r.name}: ${passes(r) ? "PASS" : "FAIL"} ${r.error ?? ""}`);
  console.log(`${rows.filter(passes).length} of ${rows.length} pass`);
  if (!a.length && !self) writeFileSync(reportPath, formatRun(rows, new Date().toISOString().slice(0, 10), signoff));
  process.exit(rows.every(passes) ? 0 : 1);
}

if (import.meta.main) main();
