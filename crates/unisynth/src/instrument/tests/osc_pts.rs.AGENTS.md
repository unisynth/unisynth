# src/instrument/tests/osc_pts.rs (sidecar: full detail of the AGENTS.md row)

Pull-only (not auto-injected).

- Point oscillator (add-tf3-import): step cycle = square with two plateaus (decimation rings at edges), linear = triangle (no even harmonic, 3rd at 1/9), LFO Scale route on a point value moves the cycle between segments, layers summed before the clip, `need` = one `Pts` per voice. v3 patches (`lfo_1hz_v3`/`route_v3` from `tests.rs`). Needs `us-osc-pm`, `us-osc-points`, `us-chunk-wide`.
- See change: add-tf3-import (tasks 3.1–3.3).
