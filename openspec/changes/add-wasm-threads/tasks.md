# Tasks

## 1. Spike and measurement

- [x] 1.1 Spike (D2): build a minimal threaded module (shared memory, atomics, `build-std`, exported `__stack_pointer` / `__wasm_init_tls`), start 3 Workers in headless Chrome that set stack + TLS, each increments a shared atomic counter 1e6 times and uses a thread-local; verify the total and per-thread TLS values; record the working flags or switch to the D2 fallback before continuing (works without wasm-bindgen: `research/wasm-threads-spike/README.md`)
- [x] 1.2 Measure live load: worst and mean `render(128)` ms of `engage` and `elysian` in Chrome with the current `synth.wasm` (extend `packages/daw/bench/dense.ts`); record in `research/size-threads/README.md` with the 2.9 ms budget; verify the numbers are reproducible across 3 runs

## 2. Pool split (song)

- [x] 2.1 Split `crates/song/src/workers.rs` into `Pool` (`new(helpers)`, `par()`, `helper()`, `stop()`) and `with_workers` (std spawner) per D3; existing workers tests + `tests/threads.rs` + `song render --threads` WAV comparison unchanged; verify `cargo test -p song`
- [x] 2.2 Test `Pool` with manually spawned std threads calling `helper()` (the WASM usage shape): every job once, barrier, stop returns helpers; verify the new test passes

## 3. Threaded engine module (synth-wasm)

- [x] 3.1 Feature `threads` on `synth-wasm` (→ `song/threads`); exports `alloc_stack`, `alloc_tls`, `pool_helper`, `bounce_open_par(p, n, helpers)`, `bounce_step_par`, `bounce_close`; native unit test: bounce_par == bounce for the demo song with std threads as helpers; verify `cargo test -p synth-wasm --features threads`
- [x] 3.2 `buildSynthWasmMt()` (beside `buildSynthWasm`) in `packages/cli/src/synth.ts` (D1 flags, `target/wasm-mt`); build succeeds and the module imports a shared memory and exports the D2 symbols; verify the default `synth.wasm` exports are unchanged (export list diff) and its demo bounce is identical
- [x] 3.3 Run the `doubt-driven-review` skill on the thread start + pool lifetime (stacks, TLS, `static mut` access only from the bounce Worker, helper exit); apply findings (→ design D4b: fresh memory per attempt, start order, same imports, checks)

## 4. DAW bounce (packages/daw, packages/cli)

- [x] 4.1 Server headers (D6) in `packages/cli/src/server.ts`; `music.test.ts`: page has COOP/COEP, assets have CORP, foreign-origin API refusal unchanged; verify tests pass
- [x] 4.2 `build.ts`: emit `synth-mt.wasm` and `bounce-helper.js`; `engine.ts` fallback ladder (D5) and helper lifecycle (D4: start, ready timeout, cancel/error terminates all); `bounce-worker.ts` threaded path (page side in new `src/bounce-mt.ts`, helper `src/bounce-helper.ts`); verify the build and `bun test packages/daw`
- [x] 4.3 Browser tests (new `test/threads.test.ts`, so `browser.test.ts` stays as is): isolated page bounces with > 1 worker and the WAV equals the single-thread bounce and `song render` (whole song + loop range, a dense project song); non-isolated page (no headers) falls back silently; cancel terminates all workers and a new bounce works; a failing helper falls back
- [x] 4.4 Measure bounce time single vs threaded (2/4/8 workers) for `engage`, `elysian`, demo in Chrome; tune WASM spin/park thresholds if needed; record in `research/size-threads/README.md`; verify threaded is faster on the dense songs
- [x] 4.5 Preview host: document the required headers for the GCS backend bucket in `packages/daw/AGENTS.md` / `publish-preview.sh` comment; verify the static build still works without them (fallback)
- [x] 4.6 Docs: `docs/synth/daw.md` (Threads row: bounce), `docs/architecture.md` threads section, directory `AGENTS.md` rows for every touched file; verify rows match the code

## 5. Integration gate

- [x] 5.1 Full suite: `cargo test --workspace`, `cargo test -p song --features threads`, `cargo test -p synth-wasm --features threads`, `bun test`; all pass (verified in a clean worktree at HEAD + this change only: another session's in-progress live-view work rebuilds the shared `target/` wasm meanwhile; there 139 pass, the only failure is the unbuilt `tools/shadertoy` `st` binary of the fresh worktree)
- [x] 5.2 Intro size unaffected: `research/size-threads/hashes.sh` equals `baseline.txt` (no synth source change) or the change does not touch player-path crates (it does not: only `song`, `synth-wasm`, `packages/*`)

## Workflow follow-up

- Decide on threaded live playback (render-ahead Worker + SharedArrayBuffer ring buffer) from the 1.2 measurement; separate change if needed.
- Archive after review.
