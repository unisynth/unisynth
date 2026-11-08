# tests/reference3.rs (sidecar: full detail of the AGENTS.md row)

Pull-only (not auto-injected).

- Env-gated (`GLORP_REF_ROOT` = `ws-intros/out`; unset ⇒ explicit skip, set + any fixture missing ⇒ panic naming the file; `load` unit-tested without the variable): song device 0 (the one ID 3 instance; params from `389811-2.bin`, no recorded automation: asserted, after asserting the CSV parser yields > 0 param rows overall) replayed from block 0 over `taps/dev00.tap` (600 blocks) **and the whole song** `taps3/dev00.tap` (78,292 blocks, 320,684,032 bytes, streamed; `tapspec` rows asserted), 13 synthetic vectors `id3ref/vec01..13` (every mode, 2x/4x, boundaries, ceiling 0, degenerate knee, NaN/inf, automation, denormals).
- Bound `1e-7 + 3e-7·|ref|` (measured max 1.19e-7: 36 / 20.0 M song samples and 5 vector samples are 1–2 ulp off, C-library `powf`); asserts the aux pair untouched by the original; NaN must match NaN. Negative controls via `must_mismatch`: flipped bit, 1/255 nudges of P0/P1/P3/P5..P10 (P2, P4 asserted inert), dropped automation (vec10), ignored mode switch (vec11), averaging off (vec04).
- Always-on: `missing_fixtures_are_a_hard_error`, `truncated_tap_is_a_hard_error`, `chunk_parser_refuses_a_song_without_exactly_one_id3`.
