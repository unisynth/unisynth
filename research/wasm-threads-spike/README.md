# wasm-threads-spike

Spike for change `add-wasm-threads` (design D2, task 1.1): a shared-memory `wasm32-unknown-unknown` module whose Web
Workers start their own stack and thread-local storage **without wasm-bindgen**. Standalone workspace (never in the
root workspace). `.gitignore`: `target/`.

## Build

```bash
RUSTFLAGS="-C target-feature=+atomics,+bulk-memory,+mutable-globals \
  -C link-arg=--shared-memory -C link-arg=--import-memory -C link-arg=--max-memory=67108864 \
  -C link-arg=--export=__stack_pointer -C link-arg=--export=__wasm_init_tls \
  -C link-arg=--export=__tls_size -C link-arg=--export=__tls_align -C link-arg=--export=__tls_base" \
cargo build --release --target wasm32-unknown-unknown -Z build-std=std,panic_abort
bun run.ts      # serves web/ with COOP/COEP, runs it in headless Chrome
```

The module imports `env.memory` (`shared: true`, max from `--max-memory`) and exports `__stack_pointer` (mutable),
`__tls_base`, `__tls_size`, `__tls_align`, `__wasm_init_tls`.

## Thread start (works)

- Leader Worker: instantiates the module with the page-created `WebAssembly.Memory({shared: true})`; its stack and TLS
  are the module defaults (initialised by the start function). It allocates per helper a 1 MiB stack
  (`alloc_block(1 MiB, 16) + 1 MiB` = top) and a TLS block (`alloc_block(__tls_size, __tls_align)`).
- Helper Worker: instantiates the same module with the same memory (memory init runs once), then **before any other
  call** `exports.__stack_pointer.value = top; exports.__wasm_init_tls(tls)`.

## Result (Chrome, headless, 2026-10-28)

`crossOriginIsolated: true`; leader + 3 helpers each did 1 000 000 `AtomicU32::fetch_add` → total 4 000 000;
each thread's `thread_local!` kept its own id (100, 1, 2, 3); recursion on each helper's stack OK, `__stack_pointer`
back at the stack top after the call; the leader blocked in a std `Condvar` (`memory.atomic.wait32`) until all
helpers reported. `__tls_size` = 12, align 4.
