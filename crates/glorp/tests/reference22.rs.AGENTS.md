# tests/reference22.rs (sidecar: full detail of the AGENTS.md row)

Pull-only (not auto-injected).

- Env-gated (`GLORP_REF_ROOT` = `ws-intros/out`; unset ⇒ explicit skip, set + any fixture missing ⇒ panic naming the file): song device 7 (the one ID 22 instance; params from `389811-2.bin`, exactly one asserted as song device 7) replayed from block 0 over the whole-song tap `id22impl/taps/dev07.tap` (78,292 blocks, 469,752 recorded writes, 30 notes; automated P0/P1/P2/P3/P6/P8) and 56 synthetic vectors `id22impl/vec/` (`manifest.json` count asserted: 26 scenarios + 30 sweeps + 3 `unsafe_*`; `vec13` 44 of 300 refused, `vec07` 16 voices); fixture bound `max err ≤ 1e-6·max|ref|` (measured: song 1.19e-7 at peak 1.92, 99.9997 % exact; vectors ≤ 9.5e-7 at peak 11.06).
- `outside_the_weight_guard_the_original_returns_nan_and_the_crate_refuses`: the three `unsafe_*` taps hold NaN (11,358 / 11,360 / 32 samples) and `check` / `Id22::new` refuse their parameters with `Weights`.
- Negative controls via `must_mismatch`: flipped reference bit, 1/255 nudges of every parameter on three tight-bound control vectors (`vec01`, `vec02`, `s07lo`; P4 by one iteration, P11 across 0.5), offsets dropped, note-offs dropped, same-offset order reversed, lane ramps dropped, song automation dropped.
- Always-on: `missing_fixtures_are_a_hard_error`, `a_set_but_missing_root_is_a_hard_error`, `short_tap_is_a_hard_error`, `event_parser_rejects_malformed_rows`, `chunk_parser_refuses_a_song_without_exactly_one_id22`.
