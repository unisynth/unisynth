# tests/reference21.rs (sidecar: full detail of the AGENTS.md row)

Pull-only (not auto-injected).

- Env-gated (`GLORP_REF_ROOT` = `ws-intros/out`; unset ⇒ skip, set + any fixture missing ⇒ panic naming the file): song devices 5 and 6 (the two ID 21 instances, params from `389811-2.bin`; 167 / 31 note-ons, 31,374 / 8 param writes asserted) replayed from block 0 over the whole-song taps `id21impl/taps/dev05.tap` / `dev06.tap` (78,292 blocks, every recorded note and param write from `ev.csv`), 40 synthetic Wine vectors `id21impl/vec/` (14 scenarios + 26 single-parameter sweeps, `manifest.json` count asserted) and the `diag/` set.
- Parity bounds per fixture: `song_rel` 4e-4 of peak for device 5 (measured 2.6e-4), 1e-6 for device 6 (2.8e-8), `VEC_REL` 1e-6, `VEC04_REL` 9e-2 for the 512-partial extreme `vec04`; tolerance, not bit parity.
- Negative controls via `must_mismatch`: flipped reference bit, 1/255 nudges of every read parameter (smallest detected printed), scheduler / event mutations, seed ulp, device 6 P7 lane dropped.
- Always-on: `missing_fixtures_are_a_hard_error`, `short_tap_is_a_hard_error`, `event_parser_rejects_malformed_rows`, `chunk_parser_refuses_a_song_without_exactly_the_two_id21_devices`, `a_write_beyond_the_partial_bound_is_refused_loudly`.
