# tests/reference28.rs (sidecar: full detail of the AGENTS.md row)

Pull-only (not auto-injected).

- Env-gated (`GLORP_REF_ROOT`; unset ⇒ explicit skip, set + fixture missing ⇒ panic naming it): song devices 11..14 (asserted as the only ID 28 chunks) replayed over the whole song (78,292 blocks, recorded events) + 125 synthetic vectors (`vec/manifest.json`, expected count asserted, two legitimately silent), tolerance `PARITY_ABS` 4e-7 + `PARITY_REL` 2e-6, aux pair untouched; negative controls (flipped bit, per-parameter nudges, scheduler / event mutations, dropped song automation) and harness hardening tests.
