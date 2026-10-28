# size-threads

Gates and measurements for change `add-synth-threads` (opt-in multi-threaded song rendering).

## Size gate

`./hashes.sh` builds the i686 `no_std` size probes **without** the `threads` feature (`ws-size-probe` `all` /
`slaughter`, `us-size-probe` all modules on the converted `showcase` song, and one `us-size-probe` per
`project/music/*.song.json` with exactly its `synth-convert features`) and prints each executable's SHA-256. `/Brepro`
replaces the link timestamp with a content hash, so equal code gives equal bytes (checked: two builds after `touch`ing
the crates give the same hashes).

`baseline.txt` = the probes of commit `dc6bb80` (threads code present, feature off) built in this checkout
(`/Users/robson/Project/64k`). Use it as a regression reference: `./research/size-threads/hashes.sh | diff
research/size-threads/baseline.txt -` must print nothing while no synth source changes. Any later synth change moves the
hashes; refresh the file in the same commit.

**The bytes depend on the checkout path.** Cargo derives symbol hashes from the absolute path of path dependencies, and
the linker orders code by symbol, so the same commit built in another directory (e.g. a `git worktree`) gives
different `us-*` executables of the same size (~900 bytes differ). Compare hashes only between builds in the same
directory.

### Isolated "feature off adds nothing" proof

Run in one clean worktree, before and after removing the threads code, so both builds share the path:

```bash
git worktree add --detach /tmp/wt HEAD
(cd /tmp/wt && ./research/size-threads/hashes.sh > /tmp/A.txt)
(cd /tmp/wt && git revert --no-commit dc6bb80 ab4523a)  # resolve doc-only conflicts with --ours; restore hashes.sh
(cd /tmp/wt && ./research/size-threads/hashes.sh > /tmp/B.txt)
diff /tmp/A.txt /tmp/B.txt                               # must print nothing
```

Results: task 5.1 at `ebf516c` ± the change: 11/11 identical, the all-modules probe imports only `kernel32!ExitProcess`.
Re-run at `dc6bb80` (after `shrink-unified-synth` landed): 11/11 identical.

## Benchmark

Benchmark songs are the dense converted songs in `project/music` (unified synth only, many tracks with buses and
sidechain receives). Single-thread wall time of `song render <song> -o x.wav` (release, before the change, Intel
i9-9900K, 8 cores / 16 threads, macOS):

| Song | Tracks | Receives | Time (s) |
|---|---|---|---|
| `engage` | 42 | 63 | 52.9 |
| `elysian` | 32 | 42 | 40.3 |
| `turtles` | 20 | 20 | 47.3 |
| `no-remorse` | 16 | 16 | 29.8 |
| `eladamri` | 9 | 9 | 14.1 |

## Parallel render (after the change)

`./bench.sh` renders each song with no flag and `--threads 1/2/4/8/16` (release, same machine), times the whole CLI
call (export excluded: the blob is exported first) and compares every WAV byte for byte with the single-thread one:

| Song | flag none | 1 | 2 | 4 | 8 | 16 | best speed-up | WAVs equal |
|---|---|---|---|---|---|---|---|---|
| `engage` | 48.3 | 46.4 | 25.9 | 14.5 | 10.7 | 12.9 | 4.51× | yes |
| `elysian` | 35.6 | 36.7 | 23.5 | 15.5 | 13.9 | 17.2 | 2.57× | yes |
| `turtles` | 42.1 | 41.7 | 23.0 | 15.4 | 11.9 | 13.8 | 3.54× | yes |
| `no-remorse` | 26.7 | 27.0 | 15.8 | 10.0 | 8.3 | 9.8 | 3.20× | yes |
| `eladamri` | 12.7 | 12.6 | 8.2 | 7.5 | 7.4 | 10.2 | 1.71× | yes |

- `--threads 1` is the old single-thread path: same time as before the change within run-to-run noise (± ~5 % on
  this machine; the "before" column above was measured with `/usr/bin/time` including export).
- Best at 8 workers = physical cores. 16 is slower: helpers share cores with busy workers (hyperthreads).
- Speed-up is bounded by the heaviest track in each dependency level (load imbalance) and by one barrier per level
  per 100-frame sub-block; `eladamri` (9 tracks) has little to spread.
- Wait policy of the std pool (`crates/song/src/workers.rs`): spin 64, then up to 2000 `yield_now`, then park. Tried
  on `engage`/`elysian` at 8 / 16 workers: busy spin 2¹⁴ gives 9.6–10.3 s / 16–63 s (16 workers collapse), 256 spin
  10.4 / 14.0 s, 64 + 2000 yields 9.4 / 10.9 s.

## Live load in the browser (add-wasm-threads task 1.2)

`SONG=project/music/<song>.song.json bun packages/daw/bench/dense.ts`: the first 30 s of the song through the live
host's `render(128)` (single-thread `synth.wasm`, Chrome, page main thread, same machine), 3 runs each. Budget =
128 frames at 44.1 kHz = 2.902 ms.

| Song | mean ms | p50 | p99 | p99.9 | max | calls > 2.9 ms (of 10 335) |
|---|---|---|---|---|---|---|
| `engage` (42 tracks) | 0.88–0.91 | 0.7–0.8 | 1.9–2.0 | 2.2–2.8 | 2.8–12.8 | 0–9 |
| `elysian` (32 tracks) | 1.32–1.35 | 1.2 | 3.0 | 3.2 | 9.4–12.7 | 296–378 |

- Mean load is 30–46 % of the budget, but `elysian` exceeds the budget on ~3 % of calls (p99 103 %): dense sections
  (heavy chords + reverbs) would drop out live on one thread. The isolated max spikes (7–13 ms) appear in single runs
  and are machine pauses (see `crates/synth-wasm/examples/spikes.rs` notes).
- This argues for threaded live playback for the densest converted songs (follow-up change, render-ahead Worker),
  not for the bounce-only scope of add-wasm-threads.

## DAW bounce in the browser (add-wasm-threads task 4.4)

`bun research/size-threads/bench-wasm.ts`: whole-song bounce in headless Chrome through `Engine.bounce` (1 worker =
the single-thread `synth.wasm` path; 2/4/8 = `synth-mt.wasm` with that many workers), seconds, same machine. All
outputs per song are identical (SHA-256 of the f32 samples).

| Song | 1 worker | 2 workers | 4 workers | 8 workers | best speed-up | equal |
|---|---|---|---|---|---|---|
| `demo` | 0.7 | 0.7 | 0.9 | 0.7 | 1.05× | yes |
| `eladamri` | 19.8 | 15.0 | 14.0 | 14.2 | 1.41× | yes |
| `elysian` | 232.1 | 126.4 | 86.0 | 70.7 | 3.28× | yes |
| `engage` | 333.2 | 176.5 | 104.7 | 89.1 | 3.74× | yes |

- WASM single-thread is ~6–7× slower than native here (`elysian` 232 s vs 36 s; no SIMD in the module) and slower
  than real time over the whole song (`elysian` ≈ 218 s long, `engage` ≈ 219 s): live playback of these songs on one
  thread cannot keep up in their dense sections, which matches the p99 overruns of task 1.2.
- Pool wait policy unchanged from native (spin 64 → 2000 yields → park; `yield_now` is a no-op on wasm, so helpers
  spin ~2000 iterations before parking). Speed-up is in line with native (3.3–3.7× at 8 workers), so no WASM-specific
  thresholds.
