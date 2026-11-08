# tests/reference25.rs (sidecar: full detail of the AGENTS.md row)

Pull-only (not auto-injected).

- Env-gated (`GLORP_REF_ROOT` = `ws-intros/out`; unset ⇒ skip, set + any fixture missing ⇒ panic): song device 8 (params from the `389811-2.bin` chunk, ID 25) replayed from block 0 over the whole-song tap `glorp-win64/analysis/harness/out/id25impl/taps/dev08.tap` (78,292 blocks, every recorded `SetParam` and note event from `ev.csv`), 44 synthetic Wine vectors `id25impl/vec/vec01..14` + 30 sweeps (`manifest.json` count asserted).
- Parity bound per fixture `max err ≤ 2.5e-3·max|ref|` (`Tol::Parity`; measured 6.857e-4 song, 1.227e-2 worst vector); negative controls use the tight per-sample bound `1e-7 + 3e-7·|ref|` (`Tol::Tight`) on `CONTROL_VECTORS` `vec02`/`vec05` only (`control_vectors_meet_the_tight_bound` asserts their baseline; NaN must match).
- Negative controls via `must_mismatch(what, …)`: flipped bit (`vec02`), 1/255 nudges of P0/P2/P3/P5..P14 (P1/P4/P8 inert; caught on at least one control vector each), scheduler mutations (offsets dropped, note-offs dropped on `vec14`, same-offset order reversed), song automation dropped.
- Always-on: `missing_fixtures_are_a_hard_error`, `short_tap_is_a_hard_error`, `event_parser_rejects_malformed_rows`, `chunk_parser_refuses_a_song_without_exactly_one_id25`.
