# src/instrument/tests/flt_noise.rs (sidecar: full detail of the AGENTS.md row)

Pull-only (not auto-injected).

- Noise + filters through the voice: level law, L/R decorrelation, band peak near 1 kHz, voice pan, level route, per-note streams + call-split/reset, filters off = bypass, SVF/ladder 1 kHz low-pass ≥ 9 dB, slots 1–4 equal and in series, each type's tilt, resonance laws, Scale/Offset cutoff + resonance routes, type automation mid-note.
- Needs `us-osc-pm`, `us-noise`, `us-flt`.
