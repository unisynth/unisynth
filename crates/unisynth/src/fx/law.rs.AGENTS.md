# src/fx/law.rs (sidecar: full detail of the AGENTS.md row)

Pull-only (not auto-injected).

- Effect laws kept as in the original engines: `exp2f`, `exp10f`, `pow2f`, `ms_to_len`, `db_to_scalar` (2^(dB/6)), `param_to_frequency/q/db`, Tunefish `eq_gain`, WaveSabre `fast_sin/cos` (512-entry table, entries computed on the spot: its error shifts low resonant corners, so Leveller keeps it).
