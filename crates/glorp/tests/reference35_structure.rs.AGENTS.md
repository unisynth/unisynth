# tests/reference35_structure.rs (sidecar: full detail of the AGENTS.md row)

Pull-only (not auto-injected).

- Env-gated (`GLORP_REF_ROOT` + `id35vec/coefsong.bin`, `coef05.bin` from `GLORP_DUMP_COEF`; unset root => skip, set root with a dump missing => fail loudly): stage loop fed with the original's own per-block coefficients is bit-exact on the song window (153,600) and vec05 (38,400) → residual error is only coefficient derivation (C-library ulps); negative control flips one coefficient ulp (6,603 / 38,400 exact).
