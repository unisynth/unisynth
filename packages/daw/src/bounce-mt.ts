// Threaded offline bounce on the page side (change add-wasm-threads, design D4/D4b/D5). Used only when the page is
// cross-origin isolated and `synth-mt.wasm` compiles. Per attempt: a fresh shared memory, one bounce Worker
// (`bounce-worker.js` with `memory`), `helpers` helper Workers (`bounce-helper.js`) started after the bounce Worker
// has opened the bounce and allocated their stacks. Any failure before the result terminates every Worker of the
// attempt and resolves `done` with `null`, so the caller falls back to the single-thread bounce.

/** Shared-memory maximum of `synth-mt.wasm` in 64 KiB pages; must equal `MT_MAX_MEMORY` in packages/cli/src/synth.ts. */
export const MT_PAGES = 16384;
/** Upper bound for the worker count (bounce Worker + helpers). */
export const MAX_WORKERS = 8;
/** A helper that has not reported `ready` within this time aborts the attempt. */
export const READY_MS = 2000;

/** Test hooks: `helperUrl` replaces the helper script (fault injection), `workers` forces the worker count. */
export type MtOptions = { helperUrl?: string; workers?: number };

let compiled: Promise<WebAssembly.Module | null> | null = null;

/** `synth-mt.wasm`, compiled once; null when not isolated or it does not compile. */
export function mtModule(base: string): Promise<WebAssembly.Module | null> {
  if (!globalThis.crossOriginIsolated) return Promise.resolve(null);
  compiled ??= WebAssembly.compileStreaming(fetch(base + "synth-mt.wasm")).catch(() => null);
  return compiled;
}

export function mtWorkers(opts: MtOptions = {}): number {
  return opts.workers ?? Math.min(navigator.hardwareConcurrency || 1, MAX_WORKERS);
}

export type MtJob = { done: Promise<Float32Array | null>; cancel: () => void; workers: number };

export function bounceThreaded(
  base: string, module: WebAssembly.Module, blob: Uint8Array, bank: Uint8Array | null, range: [number, number] | null,
  onProgress: (v: number) => void, opts: MtOptions = {},
): MtJob {
  const workers = mtWorkers(opts), helpers = workers - 1;
  const all: Worker[] = [];
  const stop = () => all.forEach((w) => w.terminate());
  let settle: (v: Float32Array | null) => void = () => {};
  let timer: ReturnType<typeof setTimeout> | undefined;
  const done = new Promise<Float32Array | null>((res) => {
    settle = (v) => {
      clearTimeout(timer);
      stop();
      res(v);
    };
  });
  const fail = () => settle(null);
  try {
    // fresh per attempt (D4b); `initial` 256 pages ≥ the module's minimum, `maximum` = the module's declared maximum
    const memory = new WebAssembly.Memory({ initial: 256, maximum: MT_PAGES, shared: true });
    const bw = new Worker(base + "bounce-worker.js", { type: "module" });
    all.push(bw);
    bw.onerror = fail;
    bw.onmessage = (e) => {
      const m = e.data;
      if (m.t === "progress") onProgress(m.value);
      else if (m.t === "done") settle(m.samples);
      else if (m.t === "error") fail();
      else if (m.t === "blocks") {
        let ready = 0;
        timer = setTimeout(fail, READY_MS);
        (m.blocks as [number, number][]).forEach(([stack, tls]) => {
          const h = new Worker(opts.helperUrl ?? base + "bounce-helper.js", { type: "module" });
          all.push(h);
          h.onerror = fail;
          h.onmessage = (e) => {
            if (e.data.t === "ready" && ++ready === helpers) {
              clearTimeout(timer);
              bw.postMessage({ t: "go" });
            } else if (e.data.t === "error") fail();
          };
          h.postMessage({ module, memory, stack, tls });
        });
      }
    };
    bw.postMessage({ module, blob, bank: bank ?? undefined, from: range?.[0] ?? 0, to: range?.[1] ?? 0, memory, helpers });
  } catch {
    fail();
  }
  return { done, cancel: () => settle(null), workers };
}
