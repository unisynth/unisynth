# src/migrate.rs (sidecar: full detail of the AGENTS.md row)

Pull-only (not auto-injected).

- Host-only migration old layout → v3 (design "v3 layout"): `values(old)`, `values_of_pairs`, `decode_old_chunk`, `instrument_chunk` (1→3, 2→4), `lane(base, index, points)` (one lane per moving v3 field; structural/era/naive-mode errors), `lane_quirks` (quirks a lane reaches;
- `lane` emits no quirk lane), `effect_chunk(k, old)` (plain mode, era → dc filter / low cut / dropped, tag), `effect_param`, `old_fx_mode(_value)`, `encode_old`, `encode_old_fx`, `is_old_instrument`, `is_old_effect`, `MigrateError`.
- Matches `research/unify-v3-layout/layout.py` on all 4,820 tracked instruments.

## History

- See change: unify-v2-algorithms 6.5–6.8
