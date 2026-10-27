# Design

## Context

- `Mixer::render_sub` (`crates/synth-core/src/mix/mixer.rs`) runs one sub-block (`DEFAULT_SUB` 100 frames) track by
  track in `order` (upstream single-thread passes). Per track: events → automations → clear → receives → devices in
  chain order → volume. Device state is reached through one `&mut [D]` indexed by song-level device indices.
- `Ctx.rng` is one `&mut dyn Rng` shared by every device. `unisynth` and `tfsynth` draw from it only at load (each
  device seeds its own `Xorshift`); `wavesabre` draws while rendering (`slaughter.rs`, `falcon.rs` via `synth.rs`),
  copying upstream's single global `rand()`.
- `denormal::Guard` sets FTZ/DAZ (x86 MXCSR) or FPCR.FZ (AArch64) for one render call. These registers are per thread.
- `synth-core` is `no_std`, no heap, MIT; player-path crates must not grow. Size is measured with
  `research/us-size-probe` and `research/ws-size-probe` (i686 `.text`, xz, squishy).

## Goals / Non-Goals

**Goals:** bit-identical parallel render of unified-synth songs on std hosts; zero bytes and zero code paths added when
the feature is off; a measured speed-up on multi-track songs.

**Non-Goals:** splitting one track (or one device's voices) across threads; pipelining devices; threaded WASM; OS
threads inside the `no_std` player; changing any song's sound.

## Decisions

### D1. Cargo feature, everything behind `#[cfg(feature = "threads")]`
`synth-core/threads` (off) gates every new item: the level table, the fork-join trait, the parallel step, the
parallel-safety check and the `Kinds` render-rng constant. The existing serial `render_sub` is not touched, so a build
without the feature compiles the same code. `song/threads` forwards it and is on for the `song` binary only.
*Alternative:* a runtime flag with the parallel path always compiled — rejected, it costs bytes in the intro.
*Alternative:* generic "executor" parameter on `render_sub` defaulting to serial — rejected, it changes the
monomorphised serial code and risks size drift.

### D2. `synth-core` does not spawn: caller-supplied fork-join
```rust
#[cfg(feature = "threads")]
pub trait Par { fn run(&self, jobs: usize, job: &(dyn Fn(usize) + Sync)); }
```
`run` returns after every `job(i)`, `i < jobs`, has finished (a barrier). `synth-core` stays `no_std`, no heap. The std
implementation in `song` is a persistent pool (threads created once per render, parked between sub-blocks, the caller
thread takes jobs too). *Alternative:* `std::thread::scope` per sub-block — rejected, ~441 spawns per second of audio per
level.

### D3. Parallel unit = track within a dependency level
At `Mixer::new` (feature on) the tracks are grouped into levels: level 0 = tracks with no receives, level k = tracks whose
senders are all in levels < k. The master is its own last level. Inside a level tracks are independent. Per sub-block:
for each level, `par.run(level_len, |i| step(track))`. Device-per-thread was rejected: a chain is serial, so it would
only pipeline with one block of delay per device and break sample-exact sidechain.

### D4. Determinism by construction
- Each track only writes its own buffers and its own devices; receives read finished buffers of earlier levels.
- Receives are summed in the track's receive order, as today, so float addition order is unchanged.
- The song random source is never handed to a worker: workers get a `Ctx` whose `rng` is a zero-returning stub, used
  only when the song is parallel-safe, i.e. no engine draws while rendering (D5).
- Events are per track (`TrackSource::peek/advance`), automations per track; the source is split into per-track handles
  so each worker owns one.

### D5. Parallel-safety check with serial fallback
Computed once at load (and after a live-style reload): every device index appears on at most one rendered track, and
`K::RENDER_RNG == false` (`unisynth` false, `tfsynth` false, `wavesabre` true, `Either` = OR of both). Otherwise the
parallel entry calls the serial `render_sub`. *As built:* automation targets are not re-checked at runtime: the blob
format resolves an automation's device within its own track's device list, and for other sources own-track automation
is part of the `unsafe trait SplitSource` contract (D8). `RENDER_RNG` is per registry, not per device instance.

### D6. Disjoint `&mut` split with `unsafe`, proven by D5
Workers need `&mut` to their track's devices and buffers. The mixer passes raw base pointers in a `Sync` wrapper; each
job reconstructs `&mut` only for indices D5 proved belong to that track alone. This is the one `unsafe` block; it gets a
`// SAFETY:` argument, a doubt-driven review and a Miri run on the host tests. Devices must be `Send` (bound only under
the feature).

### D7. Float mode per worker
Each job runs inside its own `denormal::Guard` (cheap: two register writes) so a worker thread matches the caller.
*Alternative:* set the mode once per worker thread at pool start — rejected, the pool would have to know the guard's
target mode and restore it on exit.

### D8. Source split
*As built:* a separate `unsafe trait SplitSource: Source { type Part: Send; fn parts(&mut self) -> &mut [Part] }`
(feature-gated) instead of a method on `Source`, so `Source` is unchanged. `BlobSource` already stores one `TrackSrc`
per track, so `parts` is that slice. `unsafe` because the mixer relies on the implementor's promises (one part per
track, stable routing, own-track automation) for exclusive device access. The serial path keeps calling `track(t)`.
Doubt-driven review findings applied: release range asserts on device and receive indices in the job step, and the
`NoRng` stub debug-asserts if a device declared `RENDER_RNG = false` draws anyway.

## Risks / Trade-offs

- [Barrier cost dominates on small songs: ~441 sub-blocks/s × levels] → the pool spins briefly before parking; parallel
  path is used only when a level has ≥ 2 tracks, otherwise that level runs inline on the caller.
- [Load imbalance: one heavy track bounds the level] → accepted; measured and reported, not solved here.
- [`unsafe` split wrong for a hostile song] → D5 runs on every load; hostile-song fuzz (`song/tests/hostile.rs` style)
  runs with the feature on.
- [Feature-off build silently changes] → size-probe byte comparison is a task gate, not a note.
- [WaveSabre songs get no speed-up] → by design; they are legacy, converted songs use `unisynth`.

## Migration Plan

Additive. Nothing changes for builds without the feature. Rollback = drop the feature from `song`.

## Open Questions

- Resolved: pool wait = spin 64, then up to 2000 `yield_now`, then park (measured, `research/size-threads/README.md`).
