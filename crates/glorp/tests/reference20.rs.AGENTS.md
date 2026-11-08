# tests/reference20.rs (sidecar: full detail of the AGENTS.md row)

Pull-only (not auto-injected).

- Env-gated (`GLORP_REF_ROOT` = `ws-intros/out`; unset ⇒ explicit skip, set + any fixture missing ⇒ panic naming the file): song device 4 (the one ID 20 instance, params from `389811-2.bin`) replayed from block 0 over the whole-song tap `id20impl/taps/dev04.tap` (78,292 blocks, every recorded note-on with velocity / off / `SetParam` from `ev.csv`, automated P5 / P8 / P9 / P15), 53 synthetic vectors `id20impl/vec/` (`vec/gen.py`, `manifest.json` = 23 scenarios + 32 sweeps asserted; `vec08` queue overflow, `vec05` 17 notes, `kill_*` young-voice frees, `slow213`; 2 `unsafe_*` above the ceiling run only in their own test).
- Bounds per fixture `max err ≤ rel·max|ref|`: `VEC_REL` 1e-9 (measured 0: bit-exact), `SONG_REL` 1e-4 (measured 2.4e-5; 1,499 of 78,292 blocks differ, the original's `exp` ulp).
- Negative controls via `must_mismatch`: flipped bit, nudges of every parameter except P4 (P4 nudged still matches), velocity ignored, offsets dropped, 16th note removed (17th matches), notes shifted by one, injected automation, `kill_a` one sample early.
- `above_the_velocity_ceiling_the_original_keeps_growing_and_the_crate_refuses`, `the_original_frees_a_young_voice_at_the_end_of_a_short_sub_block`.
- Always-on: `missing_fixtures_are_a_hard_error`, `short_tap_is_a_hard_error`, `event_parser_rejects_malformed_rows`, `chunk_parser_refuses_a_song_without_exactly_the_one_id20_device`.
