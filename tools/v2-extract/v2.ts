// V2 (Farbrausch) song corpus and reference renders (change add-v2-import, task 1.1). Songs are © their authors:
// everything this tool writes goes to $V2_SONGS, outside the repo. The reference player is v2redux (CC0), built
// outside the repo at $V2REDUX.
//
//   bun tools/v2-extract/v2.ts gather              copy songs from fr_public, the v2redux corpus, v2m-player (.v2mz)
//   bun tools/v2-extract/v2.ts carve <exe> [name]  depack a demo .exe and carve its first song (v2redux toolkit/era)
//   bun tools/v2-extract/v2.ts render [name…]      reference WAVs with v2dump (default: the acceptance songs)
//   bun tools/v2-extract/v2.ts survey              per-song version, features, checksum → $V2_SONGS/survey.json
//   bun tools/v2-extract/v2.ts check               acceptance checksums against v2redux PROVENANCE.md
//   bun tools/v2-extract/v2.ts oracles             build $V2_SONGS/.bin/{v2dumpjson,v2events} (cargo tests use them)
//
//   V2_LAB    synth lab dir (default ~/Downloads/64k-tools/synth-lab)
//   V2REDUX   v2redux checkout, built with cmake into build/ (default $V2_LAB/v2redux)
//   V2_SONGS  output dir (default $V2_LAB/v2songs)
// Exit 0 with a skip message when the lab or v2redux is absent.
import { createHash } from "node:crypto";
import { copyFileSync, existsSync, mkdirSync, readdirSync, readFileSync, statSync, writeFileSync } from "node:fs";
import { homedir } from "node:os";
import { basename, join, resolve } from "node:path";
import { gunzipSync } from "node:zlib";

export const LAB = process.env.V2_LAB ?? join(homedir(), "Downloads/64k-tools/synth-lab");
export const V2REDUX = process.env.V2REDUX ?? join(LAB, "v2redux");
export const SONGS = process.env.V2_SONGS ?? join(LAB, "v2songs");

/** The acceptance songs (proposal, user decision 2026-10-28): v2redux carved demo songs + the two fr_public v6 songs. */
export const ACCEPTANCE = [
  "fr08", "flybye", "fr014", "fr019", "fr022_final", "fr022_party", "fr024", "fr027", "fr029",
  "candytron", "brullwurfel", "farbomat", "kkrieger", "pzero_new", "v2_zeitmaschine_new",
] as const;

export function sha256(path: string): string {
  return createHash("sha256").update(readFileSync(path)).digest("hex");
}

/** Engine profiles of songs whose format version does not name their engine build (v2redux `eras::`): the
 * reference renders with it and the converter gets `--era <profile>`. */
export const PROFILES: Record<string, string> = {};

/** Path of an acceptance song in the corpus. */
export function acceptancePath(name: string): string {
  return join(SONGS, "acceptance", `${name}.v2m`);
}

/** Builds the JSON dumper against v2redux once; returns its path. */
export function dumper(): string {
  const out = join(SONGS, ".bin", "v2dumpjson");
  const src = join(import.meta.dir, "dump.cpp");
  if (existsSync(out) && statSync(out).mtimeMs > statSync(src).mtimeMs) return out;
  mkdirSync(join(SONGS, ".bin"), { recursive: true });
  const r = Bun.spawnSync(["clang++", "-O1", "-std=c++17", `-I${V2REDUX}/src`, src, `${V2REDUX}/src/v2load.cpp`, "-o", out]);
  if (r.exitCode !== 0) throw new Error(`building the dumper failed:\n${r.stderr}`);
  return out;
}

/** Builds the sequencer oracle (`events.cpp` + v2redux `v2seq.cpp` with stub synth entry points); returns its path. */
export function sequencer(): string {
  const out = join(SONGS, ".bin", "v2events");
  const src = join(import.meta.dir, "events.cpp");
  if (existsSync(out) && statSync(out).mtimeMs > statSync(src).mtimeMs) return out;
  mkdirSync(join(SONGS, ".bin"), { recursive: true });
  const r = Bun.spawnSync(["clang++", "-O1", "-std=c++17", `-I${V2REDUX}/src`, src, `${V2REDUX}/src/v2seq.cpp`, "-o", out]);
  if (r.exitCode !== 0) throw new Error(`building the sequencer oracle failed:\n${r.stderr}`);
  return out;
}

export interface Dump {
  error?: number;
  version: number;
  gdnum: number;
  chans: number;
  notes: number;
  pb: number;
  cc: number[];
  speech: number;
  globals: number[];
  patches: { n: number; p: number[]; m: [number, number, number][] }[];
}

/** Loader oracle: the JSON summary, and the canonical v6 bytes when `canon` is given. */
export function dump(song: string, canon?: string): Dump {
  const args = [dumper(), song, ...(canon ? ["--canon", canon] : [])];
  const r = Bun.spawnSync(args);
  if (r.exitCode !== 0) throw new Error(`${basename(song)}: ${r.stderr}`);
  return JSON.parse(r.stdout.toString());
}

const OSC = ["off", "saw/tri", "pulse", "sin", "noise", "fm", "auxA", "auxB"];
const FLT = ["off", "low", "band", "high", "notch", "all", "moogL", "moogH"];
const DST = ["off", "OD", "clip", "crush", "dec", "LPF", "BPF", "HPF", "NoF", "APF", "MoL"];
const LFO = ["saw", "tri", "pulse", "sin", "s+h"];

/** The V2 features a dumped song uses (the survey of research/v2-synth.md §2). */
export function features(d: Dump): string[] {
  const f = new Set<string>();
  for (const { p, m } of d.patches) {
    for (let i = 0; i < 3; i++) {
      const mode = p[2 + 6 * i];
      if (!mode) continue;
      f.add(`osc:${OSC[mode]}`);
      if (p[3 + 6 * i]) f.add("osc:ring");
    }
    for (const i of [0, 1]) if (p[20 + 3 * i]) f.add(`flt:${FLT[p[20 + 3 * i]]}`);
    if (p[20] && p[23] && p[26] === 2) f.add("flt:parallel");
    if (p[28]) f.add(`vdist:${DST[p[28]]}`);
    if (p[68]) f.add(`cdist:${DST[p[68]]}`);
    const src = new Set(m.filter(([, a]) => a !== 64).map(([s]) => s));
    for (const [li, b] of [[1, 44], [2, 51]] as const) {
      if (!src.has(9 + li)) continue;
      f.add(`lfo:${LFO[Math.min(p[b], 4)]}`);
      if (p[b + 2]) f.add("lfo:env");
    }
    if (p[60] || p[61]) f.add("aux:recv");
    if (p[62] || p[63]) f.add("aux:send");
    if (p[67]) f.add("boost");
    if (p[72] !== 64) f.add("chorus");
    if (p[79]) f.add("chancomp");
  }
  if (d.globals.length > 13 && d.globals[13]) f.add("sumcomp");
  if (d.speech > 1) f.add("speech");
  return [...f].sort();
}

function copy(from: string, to: string) {
  mkdirSync(resolve(to, ".."), { recursive: true });
  if (from.endsWith(".v2mz")) writeFileSync(to, gunzipSync(readFileSync(from)));
  else copyFileSync(from, to);
}

function walk(dir: string, ext: RegExp, out: string[] = []): string[] {
  if (!existsSync(dir)) return out;
  for (const e of readdirSync(dir, { withFileTypes: true })) {
    if (e.name === ".git") continue;
    const p = join(dir, e.name);
    if (e.isDirectory()) walk(p, ext, out);
    else if (ext.test(e.name)) out.push(p);
  }
  return out;
}

function gather() {
  let n = 0;
  const emb = join(V2REDUX, "corpus/embedded");
  for (const name of ACCEPTANCE) {
    const from = existsSync(join(emb, `${name}.v2m`)) ? join(emb, `${name}.v2m`) : join(V2REDUX, "corpus", `${name}.v2m`);
    if (!existsSync(from)) { console.log(`missing acceptance song ${name} (${from})`); continue; }
    copy(from, acceptancePath(name)); n++;
  }
  const fr = join(LAB, "fr_public");
  for (const p of walk(fr, /\.v2m$/i)) { copy(p, join(SONGS, "fr_public", p.slice(fr.length + 1).replaceAll("/", "__"))); n++; }
  for (const p of walk(join(LAB, "v2m-player/v2m"), /\.v2mz?$/i)) {
    copy(p, join(SONGS, "modland", basename(p).replace(/\.v2mz$/i, ".v2m"))); n++;
  }
  console.log(`gathered ${n} songs into ${SONGS}`);
}

/** Python with the toolkit's unpacker deps (unicorn, capstone): a venv in $V2_SONGS, made on first use. */
function python(): string {
  const venv = join(SONGS, ".venv");
  const py = join(venv, "bin/python3");
  if (existsSync(py)) return py;
  for (const args of [["python3", "-m", "venv", venv], [py, "-m", "pip", "install", "-q", "unicorn", "capstone"]]) {
    const r = Bun.spawnSync(args, { stdout: "inherit", stderr: "inherit" });
    if (r.exitCode !== 0) throw new Error(`${args.join(" ")} failed`);
  }
  return py;
}

function carve(exe: string, name = basename(exe).replace(/\.\w+$/, "")) {
  const era = join(V2REDUX, "toolkit/era");
  const py = python();
  const img = join(SONGS, ".bin", `${name}.image`);
  mkdirSync(join(SONGS, ".bin"), { recursive: true });
  for (const args of [["unpack", exe, img], ["carve", img, "--extract", "0", "--out", join(SONGS, "carved", `${name}.v2m`)]]) {
    mkdirSync(join(SONGS, "carved"), { recursive: true });
    const r = Bun.spawnSync([py, era, ...args], { stdout: "inherit", stderr: "inherit" });
    if (r.exitCode !== 0) throw new Error(`era ${args[0]} failed for ${exe}`);
  }
  const out = join(SONGS, "carved", `${name}.v2m`);
  console.log(`${out} sha256 ${sha256(out)}`);
}

/** Builds the solo/profile renderer (`solo.cpp` against v2redux's library); returns its path. */
export function solo(): string {
  const out = join(SONGS, ".bin", "v2solo");
  const src = join(import.meta.dir, "solo.cpp");
  if (existsSync(out) && statSync(out).mtimeMs > statSync(src).mtimeMs) return out;
  mkdirSync(join(SONGS, ".bin"), { recursive: true });
  const r = Bun.spawnSync(["clang++", "-O2", "-std=c++17", `-I${V2REDUX}/src`, src, `${V2REDUX}/build/libv2redux.a`, "-o", out]);
  if (r.exitCode !== 0) throw new Error(`building v2solo failed:\n${r.stderr}`);
  return out;
}

/** Renders the reference WAV of a song (v2redux whole song, tail trimmed; a song with a [`PROFILES`] entry renders
 * with that engine profile for the same length); returns its path. */
export function reference(song: string): string {
  const name = basename(song, ".v2m");
  const out = join(SONGS, "ref", `${name}.wav`);
  if (existsSync(out)) return out;
  mkdirSync(join(SONGS, "ref"), { recursive: true });
  const r = Bun.spawnSync([join(V2REDUX, "build/v2dump"), song, out]);
  if (r.exitCode !== 0) throw new Error(`v2dump ${song}: ${r.stderr}`);
  const profile = PROFILES[name];
  if (profile) {
    const secs = (statSync(out).size - 44) / 8 / 44100;
    const p = Bun.spawnSync([solo(), song, out, "0xffff", secs.toFixed(3), profile]);
    if (p.exitCode !== 0) throw new Error(`v2solo ${song}: ${p.stderr}`);
  }
  return out;
}

function survey() {
  const rows = [];
  for (const p of walk(SONGS, /\.v2m$/i)) {
    let d: Dump;
    try { d = dump(p); } catch (e) { rows.push({ file: p.slice(SONGS.length + 1), error: String(e) }); continue; }
    rows.push({ file: p.slice(SONGS.length + 1), sha256: sha256(p), version: d.version, error: d.error, features: d.error ? [] : features(d) });
  }
  writeFileSync(join(SONGS, "survey.json"), JSON.stringify(rows, null, 1));
  const ok = rows.filter((r) => !r.error);
  const versions: Record<number, number> = {};
  for (const r of ok) versions[r.version!] = (versions[r.version!] ?? 0) + 1;
  console.log(`${rows.length} songs, ${ok.length} parse; versions ${JSON.stringify(versions)}`);
  for (const r of ok.filter((r) => r.file.startsWith("acceptance/"))) console.log(`  ${r.file} v${r.version} ${r.features!.join(" ")}`);
}

/** Acceptance checksums against the prefixes listed in v2redux corpus/embedded/PROVENANCE.md. */
export function check(): { name: string; ok: boolean; why?: string }[] {
  const prov = readFileSync(join(V2REDUX, "corpus/embedded/PROVENANCE.md"), "utf8");
  const res = [];
  for (const name of ACCEPTANCE) {
    const p = acceptancePath(name);
    if (!existsSync(p)) { res.push({ name, ok: false, why: "missing" }); continue; }
    const m = prov.match(new RegExp("`" + name + "\\.v2m`[^\\n]*`([0-9a-f]{16})…`\\s*\\|[^\\n]*$", "m"));
    if (!m) { res.push({ name, ok: true, why: "not in PROVENANCE.md (fr_public file)" }); continue; }
    const h = sha256(p);
    res.push({ name, ok: h.startsWith(m[1]), why: h.startsWith(m[1]) ? undefined : `sha256 ${h.slice(0, 16)} ≠ ${m[1]}` });
  }
  return res;
}

if (import.meta.main) {
  if (!existsSync(join(V2REDUX, "src/v2load.h"))) {
    console.log(`skip: v2redux not found at ${V2REDUX} (set V2REDUX)`);
    process.exit(0);
  }
  const [cmd, ...rest] = process.argv.slice(2);
  if (cmd === "gather") gather();
  else if (cmd === "carve") carve(rest[0], rest[1]);
  else if (cmd === "render") for (const n of rest.length ? rest : ACCEPTANCE) console.log(reference(existsSync(n) ? n : acceptancePath(n)));
  else if (cmd === "survey") survey();
  else if (cmd === "oracles") console.log(`${dumper()}\n${sequencer()}\n${solo()}`);
  else if (cmd === "check") {
    const r = check();
    for (const x of r) console.log(`${x.ok ? "ok  " : "FAIL"} ${x.name}${x.why ? ` (${x.why})` : ""}`);
    process.exit(r.every((x) => x.ok) ? 0 : 1);
  } else {
    console.log("usage: v2.ts gather | carve <exe> [name] | render [song…] | survey | check | oracles");
    process.exit(2);
  }
}
