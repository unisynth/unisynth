// The synth engine module (crates/synth-wasm) in Bun: the same Rust code as `song check` for validating songs and
// presets before /api/music writes them, plus the GM bank summary for /api/library (change add-synth-daw).
import { existsSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { spawnSync } from "node:child_process";

export type Issue = { path: string; message: string };
export type Synth = {
  validateSong(text: string): Issue[];
  validatePreset(text: string): Issue[];
  bankInfo(bytes: Uint8Array): any | null;
};

const ROOT = fileURLToPath(new URL("../../../", import.meta.url));
export const WASM = join(ROOT, "target/wasm32-unknown-unknown/release/synth_wasm.wasm");

/** Builds the engine module (cargo, wasm32) when it is missing or `force`. */
export function buildSynthWasm(force = false): string {
  if (force || !existsSync(WASM)) {
    const r = spawnSync("cargo", ["build", "-q", "-p", "synth-wasm", "--release", "--target", "wasm32-unknown-unknown"], {
      cwd: ROOT,
      stdio: "inherit",
      env: { ...process.env, PATH: `${process.env.HOME}/.cargo/bin:${process.env.PATH}` },
    });
    if (r.status !== 0) throw new Error("synth-wasm build failed");
  }
  return WASM;
}

let cached: Synth | null = null;

export function loadSynth(path = buildSynthWasm()): Synth {
  if (cached) return cached;
  const mod = new WebAssembly.Module(readFileSync(path));
  const x: any = new WebAssembly.Instance(mod, { env: { host_now_ms: () => performance.now() } }).exports;
  const call = (fn: string, bytes: Uint8Array): { ok: boolean; out: Uint8Array } => {
    const p = x.alloc(bytes.length);
    new Uint8Array(x.memory.buffer, p, bytes.length).set(bytes);
    const ok = x[fn](p, bytes.length) === 1;
    x.dealloc(p, bytes.length);
    return { ok, out: new Uint8Array(x.memory.buffer, x.out_ptr(), x.out_len()).slice() };
  };
  const json = (b: Uint8Array) => JSON.parse(new TextDecoder().decode(b));
  const issues = (fn: string, text: string): Issue[] => {
    const r = call(fn, new TextEncoder().encode(text));
    if (fn === "preset_load" && r.ok) return [];
    const v = json(r.out);
    return v.ok === false || Array.isArray(v.errors) ? (v.errors ?? []) : [];
  };
  cached = {
    validateSong: (t) => issues("song_validate", t),
    validatePreset: (t) => issues("preset_load", t),
    bankInfo: (b) => {
      const r = call("song_bank_info", b);
      return r.ok ? json(r.out) : null;
    },
  };
  return cached;
}
