# tests/reference30.rs (sidecar: full detail of the AGENTS.md row)

Pull-only (not auto-injected).

- Env-gated (`GLORP_REF_ROOT`; unset ⇒ explicit skip, set + fixture missing ⇒ panic): ID 30 song devices 15..21 replayed from block 0 over `id30impl/taps/dev{15..21}.tap` (78,292 blocks, every recorded `SetParam`, device 18 inactive in the song), bound `3e-4·peak` (measured: 15 / 16 / 17 / 21 bit-exact, 19 2.5e-13 and 20 1.414e-4 = 9.6e-5 of peak are the two deviations; no non-finite-input vector exists); 82 vectors `id30impl/vec/` (3 refused by design: `e_halves`, `u_t1`, `u_t2`; 6 identity; all bit-exact but `a_many` 4.8e-6 of peak); negative controls (flipped bit, forced parameters, 1/255 nudges, automation dropped); `GLORP_ID30_BLOCKS=n` exploration.
- Always-on: `a_set_but_unusable_root_is_a_hard_error`.
