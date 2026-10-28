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

export const WASM_MT = join(ROOT, "target/wasm-mt/wasm32-unknown-unknown/release/synth_wasm.wasm");

/** Shared-memory maximum of the threaded module: 1 GiB (16384 pages of 64 KiB). */
export const MT_MAX_MEMORY = 1 << 30;

/**
 * Builds the threaded engine module `synth-mt.wasm` (change add-wasm-threads, design D1) when it is missing or `force`:
 * feature `threads`, shared imported memory, atomics, `-Z build-std`, and the thread-start exports (`__stack_pointer`,
 * `__wasm_init_tls`, `__tls_size`, `__tls_align`). Own target dir, so the default module's build cache is untouched.
 */
export function buildSynthWasmMt(force = false): string {
  if (force || !existsSync(WASM_MT)) {
    const flags = ["-C target-feature=+atomics,+bulk-memory,+mutable-globals", "-C link-arg=--shared-memory", "-C link-arg=--import-memory",
      `-C link-arg=--max-memory=${MT_MAX_MEMORY}`, ...["__stack_pointer", "__wasm_init_tls", "__tls_size", "__tls_align"].map((s) => `-C link-arg=--export=${s}`)];
    const r = spawnSync("cargo", ["build", "-q", "-p", "synth-wasm", "--release", "--target", "wasm32-unknown-unknown", "--features", "threads", "-Z", "build-std=std,panic_abort"], {
      cwd: ROOT,
      stdio: "inherit",
      env: { ...process.env, PATH: `${process.env.HOME}/.cargo/bin:${process.env.PATH}`, RUSTFLAGS: flags.join(" "), CARGO_TARGET_DIR: join(ROOT, "target/wasm-mt") },
    });
    if (r.status !== 0) throw new Error("synth-wasm (threads) build failed");
  }
  return WASM_MT;
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
