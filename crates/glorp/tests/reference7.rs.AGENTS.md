# tests/reference7.rs (sidecar: full detail of the AGENTS.md row)

Pull-only (not auto-injected).

- Env-gated (`GLORP_REF_ROOT` = `ws-intros/out`; unset ⇒ explicit skip, set + any fixture missing ⇒ panic naming the file; `load(root)` is unit-tested without the variable): song device 1 (the one ID 7 instance; params from the `389811-2.bin` chunk, no recorded automation: asserted) replayed over its 600-block tap `taps/dev01.tap` from block 0 (153,600 samples, compression engaged on ≥ 10,000), 11 synthetic vectors `id7vec/vec01..11` (side-chain via `process_sidechain` whenever `sidechain()`; NaN/inf, extremes, per-block automation of all params, P0 toggles).
- Bound `ABS 1e-7 + REL 3e-7·|ref|` (measured: **0** everywhere; NaN must match NaN, ±inf its sign); asserts the aux pair is untouched by the original (bitwise).
- Negative controls via `must_mismatch` (must panic with the `got … want …` message, not another failure): flipped bit, 1/255 nudges of P1..P6, dropped automation (vec07), detector forced internal (vec06), reference one sample early (vec02).
- Always-on: `missing_fixtures_are_a_hard_error`, `chunk_parser_refuses_a_song_without_exactly_one_id7`.
