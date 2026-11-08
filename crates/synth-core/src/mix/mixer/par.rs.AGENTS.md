# src/mix/mixer/par.rs (sidecar: full detail of the AGENTS.md row)

Pull-only (not auto-injected). Compiled only with feature `threads`; without it nothing of the module exists (builds byte-identical to single-thread; size probes `research/size-threads`). `synth-core` creates no threads: the caller supplies workers through `Par`.

- `Par`: `run(jobs, &dyn Fn(usize) + Sync)` calls `job(i)` for each i < jobs, any order/thread, returns after all.
- `unsafe trait SplitSource: Source` (`type Part: Send`, `parts() -> &mut [Part]`): promises one part per track behaving like `track(t)`, stable `devices()`/`receives()` while the mixer is used (check runs once at `Mixer::new`), `automate` sets only devices on its own track.
- `Levels` (built at `Mixer::new`): rendered tracks grouped by dependency level (no receives = 1, level k = senders all lower, master alone last), `ends` per level, `disjoint` = no device listed on two rendered tracks. Pool use `Levels::need(tracks)` = 3 usize slices.
- `Mixer::parallel_safe()` (= `disjoint`; caller must also know no device draws from the song rng while rendering), `levels()` (iterator of track slices, lowest first), `render_sub_par(devices, src, rate, tempo, par)`: asserts `parallel_safe`; per level one job per track (1-track level inline), each job = `step`.
- `step` = serial track step, same op order incl. quiet-skip (sleep decision, `wake`, quiet bookkeeping) and `pairs` (`run_ext`) paths; own `denormal::Guard`; `NoRng` stub (debug-asserts if drawn); release `assert!`s on device and receive indices; automation walk gated like `render_sub` (`devices[0].automation()`, read once on the caller before jobs); `denormal::flush_io` after each device (wasm only).
- Output bit-identical to `render_sub` (receives summed in the same order); Miri clean.
- History: See change: add-synth-threads, add-quiet-track-skip, add-v2-import.
