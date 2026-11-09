# src/params.rs (sidecar: full detail of the AGENTS.md row)

Pull-only (not auto-injected).

- The v3 layout, 294 values (frozen, design "v3 layout" task 6.4), then the add-tf3-import point fields `PTS` 294–320 (`pt` 9 per osc slot: `COUNT_`, `MODE`..+5, `LAYERS`, `DETUNE`, `SPREAD`; `pts(i, fld)`), `COUNT` 321: `v` 0–15, `vs` voice settings 16–22, `o` 25 per osc ×3 (23–97), `n` noise 98–101, `f` 6 per filter ×4, `e` 9 per envelope ×4, `l` 8 per LFO ×2, mod slots 1–10 at `MOD` (178–217), `ctl` 218–220, `DRIVE`/`dist` 221–224, `quirk` 225–229, `ch` strip 230–260 (`is_strip`), `MOD2` slots 11–16, `cc` 285–290, `speech` 291–293; indices < `TWO_BYTE` (240) take one byte in a chunk.
- `osc_type`: expanded `Choice(25)`, id = stored position: earlier types at `2k` (same bits as the old `k/12`), `POINTS` 1, `PAD` 3, other odd ids free (`is_gap`); `is_naive` = even ids `NAIVE_SAW..=AUX_B`. `shape::PTS_POINT`/`PTS_DRIVE`, `shape::PAD_*`. Plain choices `filter_type` (11), `env_law`, `env_curve`, `lfo_sync`, `combine`;
- `v2_raw`, `choice_value`, `quirk_on`, `smp_index`, `is_raw`, `is_structural` (+ quirks), `is_load_time(values, i)` (structural, or a PAD slot's table input s0–s4: a host rebuilds the device; add-tf3-import 6.3), `DEFAULTS`.
- Submodules `old` (frozen, host), `rename` (host), `host` (names/laws).

## History

- See change: unify-v2-algorithms 6.5–6.8
- See change: add-tf3-import 2.1 (type choice 13 → 25 positions, point fields, design D3/D5; doubt-reviewed cross-model)
