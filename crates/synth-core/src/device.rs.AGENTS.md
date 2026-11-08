# src/device.rs (sidecar: full detail of the AGENTS.md row)

Pull-only (not auto-injected).

- Buffers: `MAIN_L/MAIN_R/SIDE_L/SIDE_R` = 0..3. `Io{ch:[&mut [f32];4]}`: instruments overwrite main, effects transform main and may read side, nobody writes side. Slices hold ≥ n frames; frames past n untouched.
- `Ctx{rate: f64, tempo: i32 BPM, frame: u64, rng: &mut dyn Rng}`; `frame` = song frame of the call's first frame.
- `Device` trait (dyn-compatible). Required: `param_count`, `set_param` (index ≥ count ignored), `reset`, `run(io,n,ctx)` (n ≥ 1). Defaults: `note_on/note_off(…, offset)`, `all_notes_off`, `event_lookahead` (L, 0), `run_ext` (→ `run`), `automation` (`Ok(())`), `lane_lerp` (`Mix`), `quiet_after` (`None`), `quiet_skip` (false), `wake` (no-op).
- Events: mixer delivers every not-yet-delivered event with frame < t+n+L once; a block engine with block B needs L = B−1. Velocity 0 is still a note-on (format decoders convert).
- `automation() -> Result<(), u8>` (`#[inline]`, method so the trait stays dyn-compatible): a constant `Err(module id)` drops the mixer's automation walk; `Player::new` refuses lanes with `UnsupportedModule(id)`.
- `enum LaneLerp {Mix (default), Delta}` + `lane_lerp()`: how lanes targeting this device interpolate (`math::mix` / `math::lerp`). Read once per lane at `Player::new` from the loaded device, constant for its lifetime. A forwarding wrapper must forward it (`Either` does); a custom `Source` must honour it.
- Quiet skip: `quiet_after()` = frames of silent input after which output stays silent (below −120 dBFS) in the current state; `None` = unknown, track stays awake; instrument 0 only with no voice and no queued event; a time-only device (free-running LFO heard through input) must answer `None`. `quiet_skip()` constant `false` compiles the skip path out; mixer asks the first device. `wake(skipped)` runs once before the automation of the first sub-block after a skip (settle smoothing, advance time counters).
- Extra pairs (consts, `recv`, `run_ext` always compiled; the `pairs` feature gates only the mixer buffers/routing and `Song::parse` channel validation): `OUT_PAIRS` 4, `IN_PAIRS` 2, `EXT_CH` = 12. `ext[2k..2k+2]` = output pair k+1 (cleared per sub-block, shared by the track chain, later device write replaces earlier), `ext[8..12]` = input pairs 1/2 (filled by receives). `recv::{to,from,valid(channel,pairs),encode(from,to)}`: byte 0 receiving pair (0 main, 2 side, 4/6 input pairs), byte 1 sending pair (0 main, 1–4 output pair); `valid` w/o pairs = 0 or 2 only; > 16 bits invalid.
- `EventKind::{NoteOn{note,velocity}, NoteOff{note}}`, `Event{offset,kind}`, `Step::{Event{at,kind}, Render{start,len}}`.
- `Events<N>` slot array (upstream 256 slots): `push` first free slot (full → drop, returns false), `clear`, `is_empty`, `has_note_on(n)` (note-on with offset < n), `run(n, |Step|)` fires due events in slot order then renders up to the next event / end; offsets ≥ n carry (reduced by n).
- Tests: `tests.rs` `device_tests` (doubt-reviewed, task 2.1, 2 cycles).
- History: See change: add-wavesabre-rs, shrink-unified-synth, add-v2-import, add-quiet-track-skip.
