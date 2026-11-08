# src/params.rs (sidecar: full detail of the AGENTS.md row)

Pull-only (not auto-injected).

- The v3 layout, 294 values (frozen, design "v3 layout" task 6.4): `v` 0–15, `vs` voice settings 16–22, `o` 25 per osc ×3 (23–97), `n` noise 98–101, `f` 6 per filter ×4, `e` 9 per envelope ×4, `l` 8 per LFO ×2, mod slots 1–10 at `MOD` (178–217), `ctl` 218–220, `DRIVE`/`dist` 221–224, `quirk` 225–229, `ch` strip 230–260 (`is_strip`), `MOD2` slots 11–16, `cc` 285–290, `speech` 291–293; indices < `TWO_BYTE` (240) take one byte in a chunk.
- Plain choices `osc_type` (13, `is_naive`), `filter_type` (11), `env_law`, `env_curve`, `lfo_sync`, `combine`;
- `v2_raw`, `choice_value`, `quirk_on`, `smp_index`, `is_raw`, `is_structural` (+ quirks), `DEFAULTS`.
- Submodules `old` (frozen, host), `rename` (host), `host` (names/laws).

## History

- See change: unify-v2-algorithms 6.5–6.8
