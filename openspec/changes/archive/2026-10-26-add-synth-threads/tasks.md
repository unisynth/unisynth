# Tasks

## 1. Baseline

- [x] 1.1 Before any code change, build `research/us-size-probe` and `research/ws-size-probe` (i686, all-modules and per-song) and store the executables' SHA-256 in `research/size-threads/baseline.txt` (script `hashes.sh`, `/Brepro` for reproducible bytes; per-song probes = every `project/music` song); verify the file lists every probe build
- [x] 1.2 Use the dense converted songs in `project/music` (engage 42 tracks, elysian 32, turtles, no-remorse, eladamri: buses + sidechain receives) as benchmark songs and record single-thread render time on this machine in `research/size-threads/README.md`; verify `song render` renders it

## 2. Feature gate and safety check (synth-core)

- [x] 2.1 Add feature `threads` (off) to `crates/synth-core/Cargo.toml`; add `Kinds::RENDER_RNG` (feature-gated) set in `unisynth` (false), `tfsynth` (false), `wavesabre` (true), `Either` (OR); verify `cargo check` with and without the feature, i686 `no_std` and wasm32
- [x] 2.2 Implement the level table (D3) and the parallel-safety check (D5) behind the feature; unit tests: levels for chain / fan-in / sidechain / master-only, unsafe songs (shared device, cross-track automation, WaveSabre kind) report not-safe
- [x] 2.3 Rebuild both size probes without the feature and compare to `baseline.txt`; verify byte-identical (gate: stop and fix before continuing if not)

## 3. Parallel step (synth-core)

- [x] 3.1 Add the `Par` trait (D2) and a serial test implementation; add `Source::split` (D8) for `BlobSource` and the test `VecSource`; verify existing `player_tests` still pass with the feature on
- [x] 3.2 Implement the parallel sub-block step and `Player::render_f32_par` / `render_i16_par` with serial fallback (D4–D7), one `unsafe` split with a `// SAFETY:` argument; run the `doubt-driven-review` skill on it; verify with the serial `Par`: output == `render_f32` on all `player_tests` songs
- [x] 3.3 Tests with a real threaded `Par` (std, test-only): workers 1/2/3/8 == serial bit for bit on sidechain, chain, fan-in songs, after seeks and with call sizes 1/77/100/4096; denormal-tail song equal; WaveSabre song falls back and equals serial
- [x] 3.4 Run the host tests under Miri with the feature on (`MIRIFLAGS=-Zmiri-ignore-leaks cargo miri test -p synth-core --features threads -- player_tests::par::levels player_tests::par::render_rng player_tests::par::owned_source`: the threaded, light tests — the long ones take > 1 h under Miri; leaks ignored because `Pool` never runs destructors by design; denormal guard is a no-op under `cfg(miri)`, flush test ignored there); verify no UB reported
- [x] 3.5 Update `crates/synth-core/AGENTS.md` (mixer, player, registry, Cargo rows, `See change: add-synth-threads`); verify rows match the code

## 4. std pool and CLI (song)

- [x] 4.1 Add feature `threads` to `crates/song` (forwards `synth-core/threads`, on for the `song` bin) and a persistent std worker pool implementing `Par` (caller takes jobs, short spin then park); unit test: `run` returns only after every job, jobs see distinct indices
- [x] 4.2 `song::render::render_threads(blob, bank, n)` and `song render --threads <n>` (n ≥ 1; 0 / non-number / missing → error, nothing written); `tests/cli.rs`: `--threads 1`/`4` WAV byte-identical to no flag on the demo song, bad counts fail with no output; `tests/threads.rs`: every `project/music` song takes the parallel path and equals serial (2 s, 2/5 workers, seek); whole benchmark songs compared in 4.4 (release); `wavesabre/tests/threads.rs`: WaveSabre songs fall back
- [x] 4.3 Hostile fuzz with the feature on: `tests/hostile.rs` 1500 mutated blobs that still load render through 4 workers without panic and equal the serial render; device on two tracks falls back
- [x] 4.4 Measure the benchmark song with 1/2/4/8/16 workers, record times and speed-up in `research/size-threads/README.md`; verify `--threads 1` is not slower than before (±3 %)
- [x] 4.5 Update `crates/song/AGENTS.md` (`render.rs`, `main.rs`, `Cargo.toml` rows) and `docs/synth/daw.md` / `docs/architecture.md` audio section with the threads option; verify the documented command runs as written

## 5. Integration gate

- [x] 5.1 Final size gate: rebuild both probes without the feature, byte-identical to `baseline.txt`; `nm`/import table of the all-modules probe shows no thread, sync or atomic-wait import (run in a clean worktree at `ebf516c` ± only this change's hunks, because `shrink-unified-synth` edits `unisynth` in the same tree; 11/11 identical, only import `ExitProcess`)
- [x] 5.2 Full suite: `cargo test --workspace`, `cargo test -p synth-core --features threads`, `cargo test -p song --features threads`, `bun test`; all pass (Rust suites all pass with and without `threads`; `bun test`: 144 pass, 2 skip, 0 fail across all 27 files after adding root `bunfig.toml` `test.pathIgnorePatterns` — bun 1.4 held ~17 000 directory fds while scanning target/research/corpus and its posix_spawn then failed with EBADF)

## Workflow follow-up

- Follow-up change: threaded WASM for the DAW live host and bounce (cross-origin isolation, nightly build-std, Worker ring buffer).
- Follow-up change: OS-thread `Par` for the compo player once it plays music (raw `CreateThread`/`pthread_create`, size-measured).
- Archive after review.
