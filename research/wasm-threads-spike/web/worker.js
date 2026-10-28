self.onmessage = (e) => {
  const m = e.data;
  const x = new WebAssembly.Instance(m.module, { env: { memory: m.memory } }).exports;
  if (m.t === "leader") {
    const blocks = [];
    for (let i = 0; i < m.helpers; i++) {
      const stack = x.alloc_block(1 << 20, 16) + (1 << 20);
      const tls = x.alloc_block(x.__tls_size.value, x.__tls_align.value);
      blocks.push([stack, tls]);
    }
    postMessage({ t: "blocks", blocks });
    const id = x.work(100, m.n);
    const total = x.wait_all(m.helpers + 1);
    postMessage({ t: "total", total, id, myId: x.my_id(), tlsSize: x.__tls_size.value, tlsAlign: x.__tls_align.value, tlsBase: x.__tls_base.value, sp: x.__stack_pointer.value });
  } else {
    x.__stack_pointer.value = m.stack;
    x.__wasm_init_tls(m.tls);
    const back = x.work(m.id, m.n);
    postMessage({ id: m.id, back, tlsBase: x.__tls_base.value, sp: x.__stack_pointer.value, stack: m.stack });
  }
};
