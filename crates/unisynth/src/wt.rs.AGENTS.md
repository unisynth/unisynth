# src/wt.rs (sidecar: full detail of the AGENTS.md row)

Pull-only (not auto-injected).

- Wavetable tables (feature `us-osc-wt`; port of the Tunefish generator): `Shape`, `Table` (`update` rebuilds the spectrum only when inputs change beyond 1e-5, `render(g, &mut mu)` = phase stage → `ifft512` → peak-normalise → remove mean, `wave`, `clear`), `Wt` (pool-backed per voice and slot: `Table`, `MAX_LAYERS` 10 × (left, right) phases, drift `mu`; about 10 KB), `band_bins(freq, rate)` (original band-limit scaled by 44.1 kHz / rate), `harmonics(p)` = `1 + trunc(64p)`.
- Table-range quirk kept (phase-stage multipliers within `[0, sin π/4]`).
- Tables are compared with the Tunefish generator in `synth-convert/tests/engines.rs` (within 1e-4).
