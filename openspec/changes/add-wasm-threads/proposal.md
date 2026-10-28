# Proposal

## Why

`add-synth-threads` made song rendering parallel on native hosts (`song render --threads`, 1.7–4.5× at 8 workers) but
the DAW still bounces on one thread: `synth-wasm` is built without `threads`, every engine instance has private
memory, and WASM has no `std::thread`. A DAW bounce of a dense song (`engage`, 42 tracks) takes tens of seconds; the
same multi-core speed-up should apply in the browser. Live playback is out of scope until measured: natively the
densest project song renders ~4.7× faster than real time on one core.

## What Changes

- A second, threaded engine module `synth-mt.wasm`: shared memory, atomics, `build-std`, built by a separate cargo
  invocation (own target dir) with `synth-wasm`'s new `threads` feature. The current `synth.wasm` is unchanged and
  stays the default.
- Per-thread start for engine instances that share one memory (own stack and thread-local storage per Worker), and a
  blocking helper loop exported from the module, so JS-started Workers serve the same `Par` fork-join pool as
  `song --threads`.
- The std worker pool in `crates/song/src/workers.rs` is split into the pool itself (shared state, `Par`, helper loop,
  stop) and the std thread spawner, so the WASM build reuses the pool with externally started helpers.
- Bounce: when the page is cross-origin isolated and the threaded module loads, the bounce Worker starts
  `n − 1` helper Workers (`n` = `navigator.hardwareConcurrency`, capped) and renders through the parallel player.
  Output stays byte-identical to `song render`. Otherwise the bounce runs exactly as today.
- The `packages/cli` server sends `Cross-Origin-Opener-Policy: same-origin` and `Cross-Origin-Embedder-Policy:
  require-corp` (plus `Cross-Origin-Resource-Policy: same-origin` on DAW assets) for `/daw/`. The static preview build
  documents the same headers for its host.
- A measurement of live load: worst `render(128)` time of the densest project song in Chrome, recorded, to decide
  whether threaded live playback (render-ahead Worker + shared ring buffer) is worth a later change.

Out of scope: threaded live playback (AudioWorklet), threads in the intro player, Safari-specific workarounds beyond
the single-thread fallback.

## Discipline Skills

`security-hardening` (cross-origin isolation headers on the local server and preview host), `performance-optimization`
(bounce speed measured single vs threaded; no regression of the default module), `doubt-driven-review` (per-thread
stack/TLS start on shared memory, lifetime of jobs across Workers), `code-simplification`.

## Capabilities

### New Capabilities
- `synth-wasm-threads`: the threaded engine module and its fallback — when it is used, bit-identical bounce output,
  worker lifecycle (start, cancel, errors), cross-origin isolation requirements, and unchanged single-thread behaviour.

### Modified Capabilities
<!-- none: synth-daw's "Offline bounce" (bounce == `song render`, progress, cancel) stays as is; it lives in the
     unarchived add-synth-daw change and this change does not alter its requirement. -->

## Impact

- `crates/synth-wasm`: feature `threads` (→ `song/threads`), exports `thread_init`, `pool_helper`, `bounce_step_par`
  (names final in design), shared-memory build.
- `crates/song/src/workers.rs`: pool split from the spawner (no behaviour change for `song render --threads`).
- `packages/cli/src/synth.ts` (`buildSynthWasm({threads})`), `packages/cli/src/server.ts` (headers).
- `packages/daw`: `build.ts` (`synth-mt.wasm`, helper worker bundle), `engine.ts` (module choice, bounce),
  `bounce-worker.ts`, new `bounce-helper.ts`, browser tests, `build-static.ts` / `publish-preview.sh` (headers note).
- Toolchain: nightly `-Z build-std` (already nightly with `rust-src`).
- No change to the intro player, the blob format or `synth.wasm`.
