# crates/synth-wasm/src/host.rs (sidecar: full detail of the AGENTS.md row)

Pull-only (not auto-injected).

## API (`Host`)
- `new`, `set_bank(Vec<u8>)` / `bank_bytes()` (GM bank for sampler slots; used by the next load or device rebuild), `load(blob)`, `update(blob)` → `Edit { params, rebuilt, schedules, mixer, reload }`.
- Transport: `play(frame)`, `stop`, `seek`, `set_loop(Option<(a, b)>)` (`[a, b)`), `playing`, `position`.
- Audition / params: `note_on/off(track, note, vel)`, `set_param(track, device, param, v)` (finite values only).
- Output: `render(frames ≤ MAX_FRAMES 4096)` → owned interleaved f32 (no allocation: tested with a counting allocator), `peaks()` per track + master (blocks a call touches), `load_ratio()` (0 while silenced).
- Live displays: `watch(Option<track>)` (taps on that track's first kind-64 slot only; reapplied after `load`/`update`), `probe()` flat f32 (allocates; between calls only), `probe_ptr()`, `voice_counts()` (sounding voices per device slot; diagnostics; allocates), `set_run_open_filters(on)` (listening switch → `unisynth::flt::set_run_open`; live only).

## Behaviour
- `load`: devices in song order from the song rng (`Xorshift::new(song::render::SEED)`); one `Slot` per device = device + own arena + chunk (drop order safe). Bit-identical to `Player` from frame 0 for any call size.
- `update` diffs per track (D4): chunk value-only change (instrument non-structural values; effect with same length + mode) → `set_param`; load-time values (`params::is_load_time`: structural, a PAD slot's table inputs s0–s4, add-tf3-import 6.3) → rebuild; structural / sample / kind / mode / length / chain length → rebuild the track's chain from `host_rng`; notes or automation change → schedule swap (open notes get note-off, cursor after the next sub-block + the track's event lookahead, since those events were already delivered); receives change → mixer rebuild; track count / rate / tempo → full reload keeping transport.
- `seek` / `play`: devices reset, mixer reset at the frame, held notes chased, automation seeks; no pre-roll.
- `stop`: notes off, tails ring ≤ 2 s (`TAIL_SECONDS`); stop again = silence.
- Loop: sub-block capped at `b`, events ≥ `b` held back, wrap = seek with chase at the end sample.
- Audition queue per track (capacity 128), before chase and schedule; works playing or stopped.
- `Auto` = the player's automation walk over owned points (linear `math::mix`; assumes all `UDevice` lanes interpolate that way), bit-identical. `Sched` / `TrackView` implement `Source` / `TrackSource`.
- `Slot` forwards every `Device` method incl. defaults (`run_ext`, `automation`, `lane_lerp`, `quiet_after`, `quiet_skip`, `wake`): without `run_ext` strip sends played silent live (V2 reverb/delay; fr-041 debris-main up to 18 dB quieter).
- Time source: import `env.host_now_ms() -> f64` (wasm) for `load_ratio`.
