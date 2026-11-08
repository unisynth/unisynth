# tests/reference14.rs (sidecar: full detail of the AGENTS.md row)

Pull-only (not auto-injected).

- Env-gated (`GLORP_REF_ROOT` = `ws-intros/out`; unset ⇒ explicit skip, set + any fixture missing ⇒ panic naming the file): song devices 2 and 3 (the two ID 14 chunks, asserted as the only ID 14 devices) replayed from block 0 over the whole-song taps `id14impl/taps/dev02.tap` / `dev03.tap` (78,292 blocks, every recorded note-on / off / `SetParam` from `ev.csv`; device 2 silent, device 3 16 mono notes with P2 / P3 / P17 written every block) and 97 synthetic Wine vectors `id14impl/vec/` (`gen.py`, `manifest.json` count asserted; `vec12` 44 of 300 refused, `vec08` 16 voices, two silent: `vec05`, `p22lo`).
- Bounds: song `4e-7 + 1e-4·peak` (measured 2.9e-5), vectors `1e-6 + 7e-4·peak` (worst 3.3e-4, `r08`); the gap to bit parity is the `expf` ulp (see `id14.rs`).
- Negative controls via `must_mismatch`: flipped bit, nudges of every wire but 23 (only inert 5 / 6 / 12 / 24 unnoticed), offsets / note-offs / automation dropped, transposed, mono forced to poly on the legato vectors, every `pNNlo/hi` vector with the wire forced to the other extreme (inert wires + P0 unnoticed).
- Poly vs mono cannot be told on the song (no overlapping notes, asserted by the absence of a control).
- Always-on: `missing_fixtures_are_a_hard_error`, `short_tap_is_a_hard_error`, `event_parser_rejects_malformed_rows`, `chunk_parser_refuses_a_song_without_exactly_the_two_id14_devices`.
