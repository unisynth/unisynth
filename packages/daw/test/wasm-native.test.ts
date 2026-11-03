// The engine module (wasm) against the native renderer (`song render`, 16-bit). WebAssembly cannot switch the CPU into
// flush-to-zero mode, so where the native player flushes every subnormal (denormal::Guard, FTZ+DAZ) the wasm build
// flushes only at its denormal::flush points (feedback/filter state, device outputs). A song whose state decays through
// subnormals elsewhere differs from the native render by a last-bit step in a few samples (inaudible); a song that never
// gets there is bit-identical. Native with the wasm policy equals wasm bit for bit (measured, engage).
import { afterAll, expect, test } from "bun:test";
import { existsSync, mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { spawnSync } from "node:child_process";
import { buildSynthWasm } from "../../cli/src/synth.ts";

const ROOT = fileURLToPath(new URL("../../../", import.meta.url));
const SONG = join(ROOT, "target/release/song");
const BANK = process.env.GM_DLS ?? join(process.env.HOME ?? "", "Downloads/64k-tools/synth-lab/ws/gm.dls");
const tmp = mkdtempSync(join(tmpdir(), "wasm-native-"));
afterAll(() => rmSync(tmp, { recursive: true, force: true }));
const env = { ...process.env, PATH: `${process.env.HOME}/.cargo/bin:${process.env.PATH}` };
let wasmPath = "";

function run(args: string[]) {
  const r = spawnSync(SONG, args, { env });
  if (r.status !== 0) throw new Error(String(r.stderr));
}

/** First `secs` seconds of the song: [native 16-bit WAV samples, wasm bounce samples in the same conversion]. */
function both(song: string, secs: number, bank?: string): [Int16Array, Int16Array] {
  const f = join(ROOT, `project/music/${song}.song.json`), blob = join(tmp, `${song}.bin`), wav = join(tmp, `${song}.wav`);
  run(["export", f, "-o", blob]);
  run(["render", f, "-o", wav, ...(bank ? ["--bank", bank] : [])]);
  const x = new WebAssembly.Instance(new WebAssembly.Module(readFileSync(wasmPath)), { env: { host_now_ms: () => performance.now() } }).exports as any;
  const put = (b: Uint8Array) => { const q = x.alloc(b.length); new Uint8Array(x.memory.buffer, q, b.length).set(b); return [q, b.length]; };
  if (bank) x.set_bank(...put(new Uint8Array(readFileSync(bank))));
  const total = Math.min(x.bounce_open(...put(new Uint8Array(readFileSync(blob)))), secs * 44100);
  expect(total).toBeGreaterThan(0);
  const w = new Int16Array(2 * total);
  for (let d = 0; d < total; ) {
    const n = Math.min(4096, total - d);
    const a = new Float32Array(x.memory.buffer, x.bounce_step(n), 2 * n);
    // the player's 16-bit conversion, as `song render` writes it: clamp(trunc(f32(x·32767)))
    for (let i = 0; i < 2 * n; i++) w[2 * d + i] = Math.max(-32768, Math.min(32767, Math.trunc(Math.fround(a[i] * 32767))));
    d += n;
  }
  const b = readFileSync(wav);
  let p = 12;
  while (b.toString("ascii", p, p + 4) !== "data") p += 8 + b.readUInt32LE(p + 4);
  return [new Int16Array(b.buffer.slice(b.byteOffset + p + 8, b.byteOffset + p + 8 + w.byteLength)), w];
}

function diff(a: Int16Array, b: Int16Array) {
  let n = 0, max = 0;
  for (let i = 0; i < a.length; i++) if (a[i] !== b[i]) { n++; max = Math.max(max, Math.abs(a[i] - b[i])); }
  return { n, max };
}

test("demo: the wasm bounce equals the native render bit for bit", () => {
  expect(spawnSync("cargo", ["build", "-q", "--release", "-p", "song"], { cwd: ROOT, env }).status).toBe(0);
  wasmPath = buildSynthWasm(true);
  const [native, wasm] = both("demo", 15);
  expect(wasm.length).toBe(native.length);
  expect(diff(native, wasm)).toEqual({ n: 0, max: 0 });
}, 300_000);

test.skipIf(!existsSync(BANK))("engage: subnormals the wasm build keeps move a few samples by one step, never more", () => {
  // the first difference is at 84.9 s (frame 3746094); 90 s covers it
  const [native, wasm] = both("engage", 90, BANK);
  const d = diff(native, wasm);
  expect(d.max).toBeLessThanOrEqual(1);
  expect(d.n).toBeLessThan(native.length * 1e-5);
}, 600_000);
