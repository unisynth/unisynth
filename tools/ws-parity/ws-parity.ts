// Device parity against upstream WaveSabre (opt-in, D6 step 2): builds the uncommitted upstream checkout natively on
// macOS with our shims + render-device.cpp, renders every per-device song (crates/wavesabre/tests/songs.rs) with both
// engines at one thread / 100-frame steps, and requires SNR ≥ 60 dB per song.
//   bun tools/ws-parity/ws-parity.ts            (WS_LAB = upstream checkout, default ~/Downloads/64k-tools/synth-lab/WaveSabre;
//                                                GM_DLS = Windows gm.dls → adds the Adultery songs)
// Exit 0 pass or skipped (no checkout), 1 on a device below the threshold.
import { execFileSync } from "node:child_process";
import { existsSync, mkdirSync, readdirSync, statSync } from "node:fs";
import { homedir } from "node:os";
import { join, resolve } from "node:path";
import { readWav, type Audio } from "../synth-parity/metrics.ts";

export const MIN_SNR_DB = 60;
const ROOT = resolve(import.meta.dir, "../..");

/** SNR of `ours` against `ref` over all channels (dB; Infinity when identical). */
export function snr(ours: Audio, ref: Audio): number {
  let sig = 0, err = 0;
  for (let c = 0; c < ref.ch.length; c++)
    for (let i = 0; i < ref.ch[c].length; i++) {
      const d = (ours.ch[c][i] ?? 0) - ref.ch[c][i];
      sig += ref.ch[c][i] ** 2;
      err += d * d;
    }
  return err === 0 ? Infinity : 10 * Math.log10(sig / err);
}

export function labDir(): string {
  return process.env.WS_LAB ?? join(homedir(), "Downloads/64k-tools/synth-lab/WaveSabre");
}

function sh(cmd: string, args: string[], env: Record<string, string | undefined> = process.env): string {
  return execFileSync(cmd, args, { cwd: ROOT, env, encoding: "utf8", stdio: ["ignore", "pipe", "inherit"] });
}

/** Compiles upstream (minus the ACM/Win32 GSM and GM.DLS loaders) + shims + harness, incrementally. */
export function buildUpstream(lab: string, out: string): string {
  const obj = join(out, "obj");
  mkdirSync(obj, { recursive: true });
  const gsm = sh("brew", ["--prefix", "libgsm"]).trim();
  const shim = join(ROOT, "tools/ws-parity/shim");
  const core = join(lab, "WaveSabreCore/src"), player = join(lab, "WaveSabrePlayerLib/src");
  const srcs = [
    ...readdirSync(core).filter((f) => f.endsWith(".cpp") && f !== "GsmSample.cpp" && f !== "GmDls.cpp").map((f) => join(core, f)),
    ...readdirSync(join(core, "Deprecated")).filter((f) => f.endsWith(".cpp")).map((f) => join(core, "Deprecated", f)),
    ...["SongRenderer.cpp", "SongRenderer.Track.cpp", "CriticalSection.cpp"].map((f) => join(player, f)),
    join(shim, "GsmSample_mac.cpp"), join(shim, "GmDls_mac.cpp"), join(ROOT, "tools/ws-parity/render-device.cpp"),
  ];
  const flags = ["-std=c++17", "-O2", "-msse2", "-w", `-I${shim}`, `-I${join(lab, "WaveSabreCore/include")}`,
    `-I${join(lab, "WaveSabrePlayerLib/include")}`, `-I${join(gsm, "include")}`, `-I${join(gsm, "include/gsm")}`];
  const objs = srcs.map((src) => {
    const o = join(obj, src.split("/").pop()!.replace(/\.cpp$/, ".o"));
    if (!existsSync(o) || statSync(o).mtimeMs < statSync(src).mtimeMs) sh("clang++", [...flags, "-c", src, "-o", o]);
    return o;
  });
  const exe = join(out, "render-device");
  sh("clang++", ["-o", exe, ...objs, `-L${join(gsm, "lib")}`, "-lgsm"]);
  return exe;
}

export interface Row {
  name: string;
  snr: number;
  pass: boolean;
}

export function run(): Row[] | null {
  const lab = labDir();
  if (!existsSync(join(lab, "WaveSabreCore/src"))) {
    console.log(`ws-parity: skipped (no upstream checkout at ${lab}; set WS_LAB)`);
    return null;
  }
  const out = join(ROOT, "build/ws-parity");
  const songs = join(out, "songs");
  mkdirSync(songs, { recursive: true });
  const exe = buildUpstream(lab, out);
  const bank = process.env.GM_DLS;
  sh("cargo", ["run", "-q", "--release", "-p", "wavesabre", "--example", "render_device", "--", songs, ...(bank ? [bank] : [])]);
  const rows: Row[] = [];
  for (const f of readdirSync(songs).filter((f) => f.endsWith(".bin")).sort()) {
    const name = f.slice(0, -4), up = join(songs, `${name}_up.wav`);
    sh(exe, [join(songs, f), up], { ...process.env, GM_DLS: bank });
    const s = snr(readWav(join(songs, `${name}_rust.wav`)), readWav(up));
    rows.push({ name, snr: s, pass: s >= MIN_SNR_DB });
  }
  return rows;
}

if (import.meta.main) {
  const rows = run();
  if (rows) {
    for (const r of rows) console.log(`${r.pass ? "ok  " : "FAIL"} ${r.name.padEnd(28)} ${Number.isFinite(r.snr) ? r.snr.toFixed(1) + " dB" : "identical"}`);
    if (!process.env.GM_DLS) console.log("(GM_DLS not set: Adultery songs skipped)");
    process.exit(rows.every((r) => r.pass) ? 0 : 1);
  }
}
