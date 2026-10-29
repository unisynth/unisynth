// DAW file API (change add-synth-daw, spec synth-daw "Serving and file access", design D10):
//   GET  /api/music                 list of songs, presets and exported blobs under <root>/project/music
//   GET  /api/music?path=…          one file (path relative to the repo root, e.g. project/music/demo.song.json)
//   PUT  /api/music?path=…          write one file: *.song.json and *.patch.json are validated by the engine module
//                                   (same code as `song check`) first; atomic write (temp + rename)
//   GET  /api/library               instrument library catalog (sources that are missing are left out)
//   GET  /api/bank                  the GM bank bytes (GM_DLS or the OS bank), 404 without one
// Paths are confined to <root>/project/music (no .., absolute paths or symlinks out) and to the extensions below.
// The server applies its foreign-Origin refusal to every /api/* route before these handlers run.
import { existsSync, mkdirSync, readdirSync, readFileSync, realpathSync, renameSync, statSync, writeFileSync } from "node:fs";
import { basename, dirname, join, relative, resolve, sep } from "node:path";
import { homedir } from "node:os";
import type { Synth } from "./synth.ts";

export const MAX_BYTES = 16 * 1024 * 1024;
const EXT = [".song.json", ".patch.json", ".song.bin"];

/** Resolve `p` (repo-relative) under `<root>/project/music` with an allowed extension; null otherwise. */
export function confineMusic(root: string, p: string): string | null {
  const base = resolve(root, "project/music");
  const abs = resolve(root, p);
  const rel = relative(base, abs);
  if (rel === "" || rel.startsWith("..") || rel.split(sep).includes("..") || resolve(base, rel) !== abs) return null;
  if (!EXT.some((e) => abs.endsWith(e)) || basename(abs).startsWith(".")) return null;
  if (existsSync(base) && existsSync(abs) && !realpathSync(abs).startsWith(realpathSync(base) + sep)) return null;
  // a symlinked parent directory must not lead out either
  let d = dirname(abs);
  while (d.length > base.length && !existsSync(d)) d = dirname(d);
  if (existsSync(d) && existsSync(base) && realpathSync(d) !== realpathSync(base) && !realpathSync(d).startsWith(realpathSync(base) + sep)) return null;
  return abs;
}

function kindOf(f: string): "song" | "patch" | "blob" {
  return f.endsWith(".song.json") ? "song" : f.endsWith(".patch.json") ? "patch" : "blob";
}

function list(root: string) {
  const base = join(root, "project/music");
  const out: { path: string; kind: string; size: number; mtime: number }[] = [];
  const walk = (d: string) => {
    if (!existsSync(d)) return;
    for (const e of readdirSync(d, { withFileTypes: true })) {
      if (e.name.startsWith(".")) continue;
      const f = join(d, e.name);
      if (e.isDirectory()) walk(f);
      else if (e.isFile() && EXT.some((x) => e.name.endsWith(x))) {
        const s = statSync(f);
        out.push({ path: relative(root, f).split(sep).join("/"), kind: kindOf(e.name), size: s.size, mtime: s.mtimeMs });
      }
    }
  };
  walk(base);
  return out.sort((a, b) => a.path.localeCompare(b.path));
}

/** The GM bank: `GM_DLS` when set (empty = no bank), else the OS bank, else the synth lab copy. */
export function bankPath(env = process.env): string | null {
  if (env.GM_DLS !== undefined) return env.GM_DLS && existsSync(env.GM_DLS) ? env.GM_DLS : null; // explicit (empty = none)
  const c = [ "/System/Library/Components/CoreAudio.component/Contents/Resources/gs_instruments.dls", "C:\\Windows\\System32\\drivers\\gm.dls", join(homedir(), "Downloads/64k-tools/gmdls/gm.dls")];
  for (const p of c) if (p && existsSync(p)) return p;
  return null;
}

const WS_KIND = ["Falcon", "Slaughter", "Thunder", "Scissor", "Leveller", "Crusher", "Echo", "Smasher", "Chamber", "Twister", "Cathedral", "Adultery", "Specimen"];
const WS_INSTR = new Set(["Falcon", "Slaughter", "Thunder", "Adultery", "Specimen"]);

/** WaveSabre reference songs: instrument instances (deduplicated by chunk bytes) and effect counts per song. */
function wsSongs(dir: string) {
  if (!existsSync(dir)) return null;
  const seen = new Map<string, number>();
  const songs = readdirSync(dir).filter((f) => f.endsWith(".bin")).sort().map((f) => {
    const b = readFileSync(join(dir, f));
    const dv = new DataView(b.buffer, b.byteOffset, b.byteLength);
    const tempo = dv.getInt32(0, true);
    const n = dv.getInt32(16, true);
    let o = 20;
    const instruments: { kind: string; wave?: number; dup?: boolean }[] = [];
    const effects: Record<string, number> = {};
    for (let i = 0; i < n && o + 5 <= b.length; i++) {
      const k = b[o];
      const sz = dv.getInt32(o + 1, true);
      const name = WS_KIND[k] ?? `kind ${k}`;
      if (WS_INSTR.has(name)) {
        const key = name + ":" + Buffer.from(b.subarray(o + 5, o + 5 + sz)).toString("base64");
        const e: { kind: string; wave?: number; dup?: boolean } = { kind: name };
        if (name === "Adultery" && sz >= 4) e.wave = Math.trunc(dv.getFloat32(o + 5, true)) - 1;
        if (seen.has(key)) e.dup = true;
        seen.set(key, (seen.get(key) ?? 0) + 1);
        instruments.push(e);
      } else effects[name] = (effects[name] ?? 0) + 1;
      o += 5 + Math.max(0, sz);
    }
    return { song: f.replace(/\.(als|xrns)?\.?bin$/, "").replace(/\.bin$/, "").replace(/_/g, " "), file: f, tempo, instruments, effects };
  });
  return { songs, unique: seen.size };
}

/**
 * Converted legacy entries (change add-synth-daw task 5.7b): `project/music/.library.json`, written from the local lab
 * by `cargo run -p synth-convert --example library` (git-ignored), gives each Tunefish program and each WaveSabre
 * instrument instance its song/1 devices (instrument first, then the program's insert effects). Attached as `conv`.
 */
function attachConverted(root: string, cat: Record<string, any>) {
  const f = join(root, "project/music/.library.json");
  if (!existsSync(f)) return;
  let lib: any;
  try {
    lib = JSON.parse(readFileSync(f, "utf8"));
  } catch {
    return;
  }
  for (const p of cat.tunefish?.programs ?? []) if (lib.tunefish?.[p.n]) p.conv = lib.tunefish[p.n];
  for (const s of cat.wavesabre?.songs ?? []) s.instruments.forEach((d: any, i: number) => { const c = lib.wavesabre?.[s.file]?.[i]; if (c?.devices) d.conv = c; });
}

export function library(root: string, synth: Synth | null, env = process.env) {
  const lab = env.SYNTH_LAB ?? join(homedir(), "Downloads/64k-tools/synth-lab");
  const cat: Record<string, any> = {};
  const presets = list(root).filter((f) => f.kind === "patch").map((f) => {
    try {
      const p = JSON.parse(readFileSync(join(root, f.path), "utf8"));
      return { path: f.path, name: String(p.name ?? basename(f.path)), kind: Number(p.kind) };
    } catch {
      return { path: f.path, name: basename(f.path), kind: 0, error: "unreadable" };
    }
  });
  cat.project = { presets, licence: "ours (MIT)" };
  const bp = bankPath(env);
  if (bp && synth) {
    const info = synth.bankInfo(new Uint8Array(readFileSync(bp)));
    if (info) cat.gm = { file: basename(bp), ...info, licence: "OS sound bank: referenced by index, never redistributed" };
  }
  const progDir = join(lab, "tunefish/patches/tf4programs");
  if (existsSync(progDir)) {
    const programs = readdirSync(progDir).filter((f) => /^program\d+\.txt$/.test(f)).map((f) => ({ n: Number(f.slice(7, -4)), name: readFileSync(join(progDir, f), "utf8").split(/\r?\n/)[0].trim() })).sort((a, b) => a.n - b.n);
    const modDir = join(lab, "tunefish/media/tf4modules");
    cat.tunefish = { programs, songs: existsSync(modDir) ? readdirSync(modDir).filter((f) => f.endsWith(".tfm")).sort() : [], licence: "GPL-3 reference: converted on load and re-saved as a project preset" };
  }
  const ws = wsSongs(join(lab, "ws/refs"));
  if (ws) cat.wavesabre = { ...ws, licence: "MIT reference songs: converted on load" };
  attachConverted(root, cat);
  cat.gsm = { samples: gsmSamples(root), licence: "samples stored in project songs: each song's meta gives its licence" };
  cat.repo = {
    "wavesabre-tests": ["falcon", "slaughter_automation", "slaughter_mono_legato", "thunder", "specimen_loop", "adultery", "adultery_no_bank"],
    "tfsynth-tests": ["gen_spectrum", "gen_unison_inharmonic", "gen_drive_octave_slop", "gen_fixed_freq", "gen_mono_glide", "noise_bandpass", "filter_lp_hp", "filter_bp_notch", "env_adsr_routes", "lfo_sine", "lfo_ramp_up", "lfo_ramp_down", "lfo_square", "lfo_random", "mod_wheel_self_mod"],
    licence: "ours (MIT)",
  };
  cat.counts = {
    presets: presets.length,
    gm: cat.gm?.instruments.length ?? 0,
    tunefish: cat.tunefish?.programs.length ?? 0,
    wavesabre: cat.wavesabre ? cat.wavesabre.songs.reduce((n: number, s: any) => n + s.instruments.length, 0) : 0,
    gsm: cat.gsm.samples.length,
    repo: cat.repo["wavesabre-tests"].length + cat.repo["tfsynth-tests"].length,
  };
  return cat;
}

/** GSM samples stored in the instruments of project/music songs, one entry per distinct sample (first use kept,
 * `uses` counts the rest). Length from the chunk: WAV49 blocks of 65 bytes / 320 samples at 44.1 kHz. */
export function gsmSamples(root: string) {
  const seen = new Map<string, any>();
  for (const f of list(root).filter((x) => x.kind === "song")) {
    let song: any;
    try { song = JSON.parse(readFileSync(join(root, f.path), "utf8")); } catch { continue; }
    for (const t of [...(song.tracks ?? []), song.master].filter(Boolean)) {
      (t.devices ?? []).forEach((d: any, device: number) => (d.samples ?? []).forEach((b64: string, sample: number) => {
        const hit = seen.get(b64);
        if (hit) { hit.uses++; return; }
        const c = Buffer.from(b64, "base64");
        if (c.length < 28) return;
        const comp = c.readInt32LE(0), uncomp = c.readInt32LE(4), blocks = Math.floor(comp / 65);
        const n = uncomp > 0 && uncomp < blocks * 320 ? uncomp : blocks * 320;
        seen.set(b64, { path: f.path, song: song.meta?.title ?? basename(f.path, ".song.json"), track: t.id, trackName: t.name, device, sample, bytes: c.length, seconds: n / 44100, uses: 1 });
      }));
    }
  }
  return [...seen.values()];
}

/** Handles the music routes; null for any other path. */
export async function musicApi(root: string, req: Request, url: URL, synth: () => Synth | null, env = process.env): Promise<Response | null> {
  if (url.pathname === "/api/library") return Response.json(library(root, synth(), env));
  if (url.pathname === "/api/bank") {
    const p = bankPath(env);
    return p ? new Response(Bun.file(p), { headers: { "content-type": "application/octet-stream" } }) : new Response("no GM bank (set GM_DLS)", { status: 404 });
  }
  if (url.pathname !== "/api/music") return null;
  const path = url.searchParams.get("path");
  if (path === null) {
    if (req.method !== "GET") return new Response("method not allowed", { status: 405 });
    return Response.json(list(root));
  }
  const p = confineMusic(root, path);
  if (!p) return new Response("path outside project/music or not a song, preset or blob", { status: 403 });
  if (req.method === "GET") return existsSync(p) ? new Response(Bun.file(p)) : new Response("not found", { status: 404 });
  if (req.method !== "PUT") return new Response("method not allowed", { status: 405 });
  const len = Number(req.headers.get("content-length") ?? "0");
  if (len > MAX_BYTES) return new Response("too large", { status: 413 });
  const body = new Uint8Array(await req.arrayBuffer());
  if (body.length > MAX_BYTES) return new Response("too large", { status: 413 });
  if (!p.endsWith(".song.bin")) {
    const text = new TextDecoder().decode(body);
    const s = synth();
    if (!s) return new Response("engine module unavailable: cannot validate", { status: 503 });
    const errors = p.endsWith(".song.json") ? s.validateSong(text) : s.validatePreset(text);
    if (errors.length) return Response.json({ ok: false, errors }, { status: 422 });
  }
  mkdirSync(dirname(p), { recursive: true });
  const tmp = join(dirname(p), `.${basename(p)}.${process.pid}.tmp`);
  writeFileSync(tmp, body);
  renameSync(tmp, p);
  return Response.json({ ok: true, path: relative(root, p).split(sep).join("/") });
}
