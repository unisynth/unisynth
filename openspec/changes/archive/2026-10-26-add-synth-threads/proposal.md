# Proposal

## Why

Dense songs (many tracks, hundreds of voices) render slower than real time on one core (8 retriggered 6-note chord
tracks: 1.25× real time natively, change add-synth-daw D6). Tracks that do not receive from each other are independent,
so a multi-core machine can render them at the same time. The intro must not pay for this: a build without the option
must contain no thread code at all, so the packed size stays exactly what it is today.

## What Changes

- New opt-in build option `threads` (cargo feature on `synth-core`, forwarded by `song`). Off by default. When off, no
  thread-related code, data or imports are compiled: the size probes build byte-identical binaries.
- With the option on, the shared mixer can render each level of the track dependency graph across worker threads. Each
  track still runs its devices in chain order; receives and the master are summed in the fixed order the single-thread
  renderer uses. Output is bit-identical to the single-thread render.
- `synth-core` stays `no_std`: it does not create threads. The caller supplies the workers through a small fork-join
  interface; this change ships the std implementation (scoped OS threads) in `song`.
- A song falls back to the single-thread path when it cannot run in parallel safely: a device on more than one track,
  an automation of a device on another track, or an engine that draws from the shared song random source while
  rendering (WaveSabre). `unisynth` (the DAW and intro engine) qualifies: it draws from the song random source only at
  load.
- Each worker thread sets the same denormal flush mode as the calling thread for its work, so results match.
- `song render --threads <n>` renders through the parallel path (`n` = 1 keeps the single-thread path).

Out of scope (follow-up changes): threaded WASM for the DAW live host and bounce (needs cross-origin isolation, a nightly
`build-std` wasm build and a Worker ring buffer); OS-thread workers inside the `no_std` compo player (the player has no
music yet).

## Discipline Skills

`performance-optimization` (speed-up measured before and after, single-thread path must not regress),
`doubt-driven-review` (the `unsafe` disjoint split of devices and track buffers across threads),
`code-simplification`.

## Capabilities

### New Capabilities
- `synth-threads`: opt-in multi-threaded song rendering — the build option and its zero-cost-when-off guarantee,
  bit-identical parallel output, the parallel-safety conditions with single-thread fallback, per-thread float mode, and
  the CLI worker count.

### Modified Capabilities
<!-- none: the single-thread track order (synth-mix "Track order and master") and determinism (synth-core) are unchanged
     and remain the reference the parallel path must match. -->

## Impact

- `crates/synth-core`: `Cargo.toml` feature `threads`; `mix/mixer.rs` (dependency levels, parallel sub-block step behind
  `#[cfg(feature = "threads")]`), `mix/player.rs` (parallel render entry), `mix/registry.rs` (engine declares whether it
  draws from the song random source while rendering), `device.rs` / `denormal.rs` (per-thread guard use).
- `crates/unisynth`: declares no render-time draws from the song random source.
- `crates/song`: feature `threads` (on for the CLI binary), std worker pool, `render --threads`.
- Size probes (`research/us-size-probe`, `research/ws-size-probe`): rebuilt without the feature, compared byte for byte.
- No change to the blob format, the `song/1` format, the DAW, `synth-wasm` or the player.
