# crates/synth-wasm/src/lib.rs (sidecar: full detail of the AGENTS.md row)

Pull-only (not auto-injected).

One module serves the AudioWorklet (host functions), the main thread (song functions) and the bounce Worker. Plain C ABI, no wasm-bindgen; static `HOST`, `OUT`; wasm32 single-threaded, exports not re-entrant. Bytes in via `alloc`/`dealloc`; replies (JSON or bytes) via `out_ptr`/`out_len`.

## Rust API
- Re-exports `Host`, `Edit`, `MAX_FRAMES`. `tables_json()`, `physical(kind, mode, index, p)`, `gsm_chunk_decode(chunk)` → mono f32 PCM 44.1 kHz, `gsm_chunk_encode(pcm)` → GSM sample chunk (header + WAV49 blocks, `gsm::encode`), `bank_info(bank)`.

## C ABI
- Memory: `alloc`, `dealloc`, `out_ptr`, `out_len`.
- Host: `set_bank`, `load` (1 ok / 0 error JSON in OUT), `update` (OUT `{ok, params, rebuilt, schedules, mixer, reload}`), `render(frames) → *const f32`, `play`, `stop`, `seek`, `set_loop(start, end, on)`, `position`, `is_playing`, `note_on`, `note_off`, `set_param`, `peaks_ptr`/`peaks_len` (`[L0, R0, …]`, master last), `load_ratio`.
- Probe: `probe_watch(track)` (< 0 none), `probe_len()` fills and returns length, `probe_ptr()`. Layout: header `[1, voices, 4 filter slots, 2048 tap frames]`; 22 floats per voice (`id, note, amp, released, age_s`, (env level, stage 0 attack…4 finished)×4, lfo phase×2, sampler position×3 (−1 none), cutoff Hz×4 (−1 none)); per filter slot `1` + input + output (tap frames each, oldest first) or `0`. `run_open_filters(on)`.
- Song: `song_tables`, `song_physical`, `song_validate` (OUT `{ok, errors}`), `song_migrate` (OUT = song read as song/2, or issues), `song_export` (1 = blob in OUT, 0 = issues JSON), `song_import`, `preset_save` (`{device, name}`), `preset_load`, `song_bank_info` (OUT = `bank_info` JSON: `{waves, instruments: [{name, drums, bank, prog, regions}]}`).
- GSM: `gsm_decode(p, n)` (OUT f32 LE), `gsm_encode(p, n)` (n f32 LE at any alignment → OUT chunk).
- Bounce (Worker; stepped, progress + cancel): `bounce_open(blob) → total frames` (0 on error, issues in OUT; uses the bank set with `set_bank`; the exact intro sound = `song::render::render`, D7), `bounce_step(frames)` → pointer to the next `min(frames, 4096, left)` interleaved f32, `bounce_left()`. State = `Bouncer` (player + borrowed memory, blob, bank). `#[cfg(feature = "threads")] mod par` adds the threaded variants (see `par.rs`).

See change: add-synth-daw, add-wasm-threads, add-device-live-view, unify-v2-algorithms.
