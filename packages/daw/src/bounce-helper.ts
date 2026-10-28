// Helper Worker of a threaded bounce (change add-wasm-threads, design D2/D4b): instantiates `synth-mt.wasm` on the
// bounce's shared memory with the same imports as the bounce Worker, switches to its own stack and TLS block, reports
// `ready`, then serves the bounce's pool until it closes. Only `pool_helper` is called after the start.
export {};

self.onmessage = (e: MessageEvent) => {
  const { module, memory, stack, tls } = e.data as { module: WebAssembly.Module; memory: WebAssembly.Memory; stack: number; tls: number };
  try {
    const x = new WebAssembly.Instance(module, { env: { memory, host_now_ms: () => performance.now() } }).exports as Record<string, any>;
    x.__stack_pointer.value = stack;
    x.__wasm_init_tls(tls);
    postMessage({ t: "ready" });
    if (x.pool_helper() === 0) postMessage({ t: "error", message: "no threaded bounce open" });
    else postMessage({ t: "stopped" });
  } catch (err) {
    postMessage({ t: "error", message: String(err) });
  }
};
