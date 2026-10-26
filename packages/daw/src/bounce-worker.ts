// Offline bounce (design D7): renders the exported blob through the song player in its own engine instance, in
// steps, posting progress; the page cancels by terminating the worker. Equal to `song render`.
export {};
type X = Record<string, any> & { memory: WebAssembly.Memory };

self.onmessage = (e: MessageEvent) => {
  const { module, blob, bank, from, to } = e.data as { module: WebAssembly.Module; blob: Uint8Array; bank?: Uint8Array; from: number; to: number };
  const x = new WebAssembly.Instance(module, { env: { host_now_ms: () => performance.now() } }).exports as X;
  const put = (b: Uint8Array): [number, number] => {
    const p = x.alloc(b.length);
    new Uint8Array(x.memory.buffer, p, b.length).set(b);
    return [p, b.length];
  };
  if (bank) {
    const [p, n] = put(bank);
    x.set_bank(p, n);
  }
  const [p, n] = put(blob);
  const total = x.bounce_open(p, n);
  if (!total) {
    postMessage({ t: "error", message: new TextDecoder().decode(new Uint8Array(x.memory.buffer, x.out_ptr(), x.out_len())) });
    return;
  }
  const end = Math.min(total, to > 0 ? to : total);
  const out = new Float32Array(2 * Math.max(0, end - from));
  let done = 0, last = 0;
  while (done < end) {
    const k = Math.min(4096, end - done);
    const ptr = x.bounce_step(k);
    if (done + k > from) {
      const src = new Float32Array(x.memory.buffer, ptr, 2 * k);
      const skip = Math.max(0, from - done);
      out.set(src.subarray(2 * skip), 2 * (done + skip - from));
    }
    done += k;
    if (done - last >= 44100) {
      last = done;
      postMessage({ t: "progress", value: done / end });
    }
  }
  postMessage({ t: "done", samples: out }, [out.buffer] as any);
};
