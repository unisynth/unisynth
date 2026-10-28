# Design

## Context

- `packages/daw/src/engine.ts` compiles `synth.wasm` once and passes the `WebAssembly.Module` to the AudioWorklet
  (`worklet.ts`) and to a per-bounce Worker (`bounce-worker.ts`); each makes its own instance with private memory.
  The bounce calls `bounce_open` / `bounce_step(4096)` / `bounce_left` (`crates/synth-wasm/src/lib.rs`, global
  `static mut BOUNCER`) and posts progress; cancel = `worker.terminate()`.
- `crates/synth-wasm` is a plain C ABI cdylib (no wasm-bindgen), one import `env.host_now_ms`, depends on `song` with
  `default-features = false` (no `threads`).
- `synth-core` with `threads` provides `Player::render_f32_par(out, &dyn Par)`, bit-identical to `render_f32`
  (archived change add-synth-threads). `crates/song/src/workers.rs` is a std pool: `with_workers(n, f)` spawns scoped
  helpers; `Par::run` is a barrier where every helper takes part; helpers spin → yield → park on a `Condvar`.
- WASM facts that shape the design: shared memory needs cross-origin isolation (COOP + COEP); `memory.atomic.wait32`
  (std `Mutex`/`Condvar` on wasm32+atomics) is allowed in dedicated Workers, not on the page's main thread; WASM
  globals (`__stack_pointer`, `__tls_base`) are per instance while memory is shared; `std::thread::spawn` is
  unsupported on `wasm32-unknown-unknown`.

## Goals / Non-Goals

**Goals:** threaded bounce, byte-identical output, silent fallback, no change to `synth.wasm` or the live path.

**Non-Goals:** threaded live playback; wasm-bindgen for the default module; Safari/Firefox tuning beyond "falls back
or works"; growing beyond one shared-memory maximum chosen at build time.

## Decisions

### D1. Separate artifact `synth-mt.wasm`
Built by `cargo build -p synth-wasm --release --target wasm32-unknown-unknown --features threads -Z
build-std=std,panic_abort` with `RUSTFLAGS="-C target-feature=+atomics,+bulk-memory,+mutable-globals -C
link-arg=--shared-memory -C link-arg=--import-memory -C link-arg=--max-memory=<MAX> -C link-arg=--export=__stack_pointer
-C link-arg=--export=__wasm_init_tls -C link-arg=--export=__tls_size -C link-arg=--export=__tls_align"` and
`CARGO_TARGET_DIR=target/wasm-mt` (different flags must not invalidate the default build's cache). `MAX` = 1 GiB
(16384 pages; the bounce pool of the largest project song is far below). `synth.wasm` keeps its exact build.
*Alternative:* one threaded module for everything — rejected: requires isolation everywhere (the dashboard canvas
frame is not isolated) and changes the live path.

### D2. Thread start without wasm-bindgen (spike first)
Each helper Worker instantiates the module with the same imported `WebAssembly.Memory`, then before any other call:
1. sets the exported mutable global `__stack_pointer` to the top of a stack the bounce Worker allocated for it with
   `alloc_stack(size)` (module export, 1 MiB per helper),
2. calls `__wasm_init_tls(ptr)` with a block from `alloc_tls()` (size/alignment from `__tls_size` / `__tls_align`).
Memory initialisation runs once (lld's `__wasm_init_memory` is guarded for shared memory). Task 1.1 is a spike that
proves this on Chrome with a counter test before anything else is built. *Fallback if the spike fails:* build the
threaded module with wasm-bindgen + `wasm-bindgen-rayon`'s start shim for this artifact only (changes the JS glue of the
bounce, not the specs).

### D3. Reuse the std pool: split pool from spawner
`song::workers` becomes `Pool` (shared state; `par()` → `impl Par`; `helper()` = the blocking helper loop; `stop()`)
plus `with_workers(n, f)` (std: spawns `n − 1` scoped threads running `helper()`, as today). The WASM build keeps one
`Pool` per bounce in module memory; JS-started helpers call the export `pool_helper()` which runs `helper()` until
`stop()`. The bounce Worker is the caller thread: it may block, so the existing wait policy (spin → yield → park; caller
spin → yield) is valid unchanged. `std::thread::yield_now` is a no-op on wasm; the spin/park thresholds may need wasm
values (measured in task 4.x).
*Alternative:* a separate lock-free wasm pool — rejected while the caller is a Worker (blocking allowed); needed only
for a future live path where the caller is the AudioWorklet.

### D4. Bounce flow
`bounce_open_par(p, n, helpers)` = `bounce_open` + creates the pool; returns total frames. JS starts `helpers` helper
Workers (`bounce-helper.js`), each runs D2 then `pool_helper()` (blocks). `bounce_step_par(frames)` renders through
`render_f32_par`. `bounce_close()` stops the pool (helpers return from `pool_helper`) and drops the bouncer. Cancel or
error: the bounce Worker reports, and `engine.ts` terminates the bounce Worker and all helpers (it holds their handles;
helpers are created by `engine.ts`, not nested, so the page can terminate them directly and so Safari's limited nested
worker support does not matter). Helper count = `min(hardwareConcurrency, 8) − 1`, ≥ 1.

### D4b. One shared memory per bounce attempt (doubt-driven review, task 3.3)
Locks (`Pool`'s mutexes, the allocator's lock) live in the shared memory, and `Worker.terminate()` can stop a thread
holding one. So every threaded bounce attempt uses a **fresh** `WebAssembly.Memory` and fresh instances, and any
failure or cancel discards all of them; the memory is never reused. Order: bounce Worker instantiates, calls
`bounce_open_par(blob, H)`, allocates H stacks + TLS blocks (`alloc_block`; any 0 → fallback) and posts them; only then
does `engine.ts` start the H helpers. Helpers get the same imports as the bounce Worker (`env.memory`,
`env.host_now_ms`), start (D2), post `ready`, call `pool_helper()`; a return of 0 is an error. `bounce_step_par` is
called only after all H are ready. Helper stack = 1 MiB, the same as the module's own default stack the bounce Worker
renders on (jobs run the same track step). The render path does not allocate (pool memory), so shared memory does not
grow during parallel runs; JS views are re-created per step.

### D5. Fallback ladder
`engine.ts` uses the threaded bounce only if `crossOriginIsolated`, `synth-mt.wasm` compiles, and every helper reports
ready within a timeout (2 s). Any failure → terminate the bounce Worker and every helper, drop the shared memory, and
run today's single-thread bounce in a fresh Worker with private memory (D1 of add-synth-daw unchanged). The threaded
module is compiled lazily on the first bounce, so page start cost is unchanged.

### D6. Headers
`packages/cli/src/server.ts`: `/daw/` page → `Cross-Origin-Opener-Policy: same-origin`,
`Cross-Origin-Embedder-Policy: require-corp`; every `/daw/*` asset → `Cross-Origin-Resource-Policy: same-origin`.
API routes unchanged. The DAW loads only same-origin assets (checked in tests). Preview host
(`publish-preview.sh`, GCS behind the shared LB): headers need the backend bucket's custom response headers; documented,
not automated here (fallback keeps the preview working without them).

## Risks / Trade-offs

- [Spike fails: exported `__stack_pointer` not mutable / TLS init differs in current nightly] → D2 fallback
  (wasm-bindgen for `synth-mt.wasm`).
- [Shared memory cannot grow past MAX; bounce of a huge song fails] → error reported, fallback to single-thread bounce.
- [COEP breaks a resource the page loads] → all DAW assets are same-origin; browser test asserts `crossOriginIsolated`
  and no blocked requests.
- [Barrier cost higher in WASM (no `yield_now`, futex via JS engine)] → measured; thresholds tuned; speed-up recorded.
- [`static mut` globals in `synth-wasm` are now visible to all threads] → helpers call only `thread_init` and
  `pool_helper`; all other exports are called by the bounce Worker alone (documented invariant, asserted in debug).

## Migration Plan

Additive; default module and live path untouched. Rollback = stop shipping `synth-mt.wasm` (fallback engages).

## Open Questions

- Spin/park thresholds for WASM helpers: from measurement (task 4.4).
