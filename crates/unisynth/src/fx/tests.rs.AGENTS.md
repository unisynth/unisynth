# src/fx/tests.rs (sidecar: full detail of the AGENTS.md row)

Pull-only (not auto-injected).

- Harness (`with_fx`, `run`, `values(seed)`, `chunk(k, vals)` migrating old-layout values, `old_mode_value`); framework tests; per mode: spot values, rate behaviour (ms/Hz in seconds), call-size independence, set_param = load order.
- Clocked modes: free stored positions + old values unchanged, comb nulls a sine at rate/K, sweep offset + restart, sweep = the original in-place doubler (ported; latency K+1, s = 65536/0xFF3B − 1, error −31 dB), wrap spot values.
- Quiet-skip: `quiet_after_is_the_memory_of_the_mode`, `a_woken_three_band_ramps_as_if_it_had_run`, `a_woken_comb_sweeps_on_as_if_it_had_run`, `a_freeverb_answers_its_memory`.
- Format: plain modes and old values migrate, tag-only chunk loads, old/cut chunks refused.

## History

- See change: add-cauldron-xm-import, add-quiet-track-skip, compact-cauldron-xm-import, unify-v2-algorithms 6.5–6.8
