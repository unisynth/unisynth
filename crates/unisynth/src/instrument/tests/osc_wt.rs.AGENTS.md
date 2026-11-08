# src/instrument/tests/osc_wt.rs (sidecar: full detail of the AGENTS.md row)

Pull-only (not auto-injected).

- Wavetable: single partial at bin 2 (sounds at 2× note), 1/h and 1/h² partial series, pitch at 22–96 kHz, detune p³ (left/right) and spread p⁴ (10 layers, 2 rates), drive 1+32p vs clipped-table harmonics, 2048-frame rebuild cadence + restart, unchanged shape keeps table, drift value, split/reset independence, PM send from a wavetable slot, pitch-route scales detune, pool sizing by slot type.
- Needs `us-osc-pm` + `us-osc-wt`.
