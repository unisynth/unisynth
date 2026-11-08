# src/tests3.rs (sidecar: full detail of the AGENTS.md row)

Pull-only (not auto-injected).

- Synthetic only, ID 3: rejection table + atomicity, accepted range, mode (`P4` 0.2500/0.2501/0.4999/0.5/0.5004/0.7501/0.7503) and averaging (`P5` 0.4999/0.5/0.9999/1.0) selector boundaries, closed-form clip / fold / π-2 / cubic / soft-knee values, odd symmetry, ceiling / drive / gain / mix laws, 2x / 4x averaging closed forms (no gains on sub-samples), independent f64 model, finite inputs only (NaN / inf pinned separately and against the reference vectors; 8 settings + song settings 1e-4, per-block automation), block-split bit identity, channel independence, `set_param` keeps history, parameters inert in the wrong mode, ceiling 0, NaN / inf pins, negative control: 1/255 nudges exceed 1e-4 (baseline < 1e-5).
