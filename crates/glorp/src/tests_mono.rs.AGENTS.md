# src/tests_mono.rs (sidecar: full detail of the AGENTS.md row)

Pull-only (not auto-injected).

- Synthetic only (9 tests, recording dummy voice): the shared scheduler's mono note stack: mode flag, first note starts one voice / legato none, top release falls back to the held note vs non-top release, repeated note, a dead voice is not replaced under held keys, stray note-0 release, 129th-cell depth alias, mode change clears queue + stack + voices.
