# src/mix/player.rs (sidecar: full detail of the AGENTS.md row)

Pull-only (not auto-injected). Single-thread deterministic port of upstream `SongRenderer` / `Track::Run`.

- `Player<'a, K: Kinds, R: Rng + Clone>` = `Mixer` + `BlobSource`. `required_pool(song, env, sub)`, `new(song, env, pool, rng, sub)`, `song()`, `devices()`, `length_frames()` (`length·rate` truncated), `set_skip_quiet(on)` (forwards to mixer), `render_f32` / `render_i16` (interleaved stereo; frames = len/2; caller buffers of any size served from the master's sub-block), `seek(frame)`.
- `new` order: refuse `sub != K::block(&song, env)?` (`Malformed`; `None` = any) → `load_devices` (runs `K::preflight` first) → per lane: device index in track (`BadIndex`), `automation()` `Err(id)` → `UnsupportedModule(id)`, param ≥ `param_count` → `BadIndex` → `BlobSource::new` → `Mixer::new` (track order = upstream passes, stops when master done; cycle feeding master → `BadIndex`) → `update_look`. `rng` = song random source: devices draw at load; post-load clone is what seek restores.
- `seek`: reset devices, tracks and rng snapshot, silent re-render of sub-blocks up to `frame`.
- `DEFAULT_SUB` = 100 (upstream step). Per sub-block: events (track lookahead = max device L) → automations (upstream index walk; int→f32 interpolation in the **target device's** `lane_lerp` form: `math::mix` default, `math::lerp` for `Delta`; 0 points skipped) → clear → receives (main L/R × vol into ch 0/2) → devices → volume.
- `BlobSource` (`Source` over the blob: `TrackSrc` per track = lane cursor + `AutoState` walk, deltas accumulated). `BlobSource::new(song, devices, pool)` sets `AutoState.lerp` from `devices[d].lane_lerp()` per lane (mixed engines in one song each keep their form; seek/reset keep it).
- `pub fn pcm_i16(f32) -> i16` (re-exported from `mix`) = `clamp(trunc(x·32767))`, f32 product, NaN → 0, −1.0 → −32767; used by `render_i16`/`render_i16_par` and the glorp whole-song comparison/export (pinned by `glorp/tests/reference_player.rs::pcm_conversion_is_the_upstream_truncation`).
- Feature `threads`: `unsafe impl SplitSource for BlobSource`; for `K: Send`: `parallel()` (= `!K::RENDER_RNG && mixer.parallel_safe()`), `render_f32_par` / `render_i16_par(out, &dyn Par)` (serial fallback, same output), `mixer()`.
- Measured: mixer split byte-equal on 56 lab songs; i686 probe +192 B `.text`.
- History: See change: add-synth-daw, add-synth-threads, shrink-unified-synth, add-quiet-track-skip. Glorp player wiring (`block` check, per-lane `lane_lerp`) has no openspec change.
