// Offline bounce (design D7): renders the exported blob through the song player in its own engine instance, in
// steps, posting progress; the page cancels by terminating the worker. Equal to `song render`.
//
// Threaded variant (change add-wasm-threads, design D4/D4b): with `memory` (a fresh shared WebAssembly.Memory) and
// `helpers` = H, the worker instantiates `synth-mt.wasm` on that memory, opens the bounce with a pool for H helpers,
// allocates their stacks and TLS blocks and posts them (`blocks`), then waits for `go` (all helpers started) before
// rendering through `bounce_step_par`. Any problem is posted as `error`; the page then falls back.
export {};
type X = Record<string, any> & { memory: WebAssembly.Memory };

const STACK = 1 << 20; // per helper: the module's own default stack size (D4b)
let go: (() => void) | null = null;

self.onmessage = async (e: MessageEvent) => {
  if (e.data?.t === "go") return go?.();
  const { module, blob, bank, from, to, memory, helpers } = e.data as {
    module: WebAssembly.Module; blob: Uint8Array; bank?: Uint8Array; from: number; to: number; memory?: WebAssembly.Memory; helpers?: number;
  };
  const env: Record<string, WebAssembly.ImportValue> = { host_now_ms: () => performance.now() };
  if (memory) env.memory = memory;
  const x = new WebAssembly.Instance(module, { env }).exports as X;
  const mem = () => (memory ?? x.memory).buffer;
  const put = (b: Uint8Array): [number, number] => {
    const p = x.alloc(b.length);
    new Uint8Array(mem(), p, b.length).set(b);
    return [p, b.length];
  };
  if (bank) {
    const [p, n] = put(bank);
    x.set_bank(p, n);
  }
  const [p, n] = put(blob);
  const total = memory ? x.bounce_open_par(p, n, helpers) : x.bounce_open(p, n);
  if (!total) {
    postMessage({ t: "error", message: new TextDecoder().decode(new Uint8Array(mem(), x.out_ptr(), x.out_len()).slice()) });
    return;
  }
  if (memory) {
    const blocks: [number, number][] = [];
    for (let i = 0; i < helpers!; i++) {
      const stack = x.alloc_block(STACK, 16);
      const tls = x.alloc_block(x.__tls_size.value, x.__tls_align.value);
      if (!stack || !tls) {
        postMessage({ t: "error", message: "out of memory for helper stacks" });
        return;
      }
      blocks.push([stack + STACK, tls]);
    }
    await new Promise<void>((res) => {
      go = res;
      postMessage({ t: "blocks", blocks });
    });
  }
  const step: (k: number) => number = memory ? x.bounce_step_par : x.bounce_step;
  const end = Math.min(total, to > 0 ? to : total);
  const out = new Float32Array(2 * Math.max(0, end - from));
  let done = 0, last = 0;
  while (done < end) {
    const k = Math.min(4096, end - done);
    const ptr = step(k);
    if (done + k > from) {
      const src = new Float32Array(mem(), ptr, 2 * k); // a fresh view per step: shared memory may have grown
      const skip = Math.max(0, from - done);
      out.set(src.subarray(2 * skip), 2 * (done + skip - from));
    }
    done += k;
    if (done - last >= 44100) {
      last = done;
      postMessage({ t: "progress", value: done / end });
    }
  }
  if (memory) x.bounce_close();
  postMessage({ t: "done", samples: out }, [out.buffer] as any);
};
