# src/lfo.rs (sidecar: full detail of the AGENTS.md row)

Pull-only (not auto-injected).

- One LFO for every law: `Lfo{phase}` (turns, f64: `restart`, `advance(dt, rate, one_shot)`, `value(kind)`), `LfoP::read(p, k)` (`l::{LAW, SHAPE, SYNC, PHASE, POLARITY, ONE_SHOT}`;
- `shape_of`, `rate_of`, `one_shot_of`, `view_of`, `sync_of`, `uses_settings` needs `us-lfo-frame`), `shape::{SINE, RAMP_UP, RAMP_DOWN, SQUARE, RANDOM (256 hashed steps), TRIANGLE, SAMPLE_HOLD, PULSE}` (`ALL` 8, `COUNT_V1` 5 = Hz-law set), `FRAME_SHAPES`, `sync::{RESTART,SONG_TIME,CONTINUE}`, `scale_view(depth, shape)` = `1 − d + d·shape`, `offset_view` = `d·(2·shape − 1)`, `d = depth²`.
- Frame-law sine is the table sine.
- Tests keep the earlier modes (`old` module) and compare.

## History

- See change: add-v2-import, unify-v2-algorithms 3.1, 6.5–6.8
