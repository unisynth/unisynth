# src/pts.rs (sidecar: full detail of the AGENTS.md row)

Pull-only (not auto-injected).

- Point oscillator (`us-osc-points`, add-tf3-import; TF3 spec §3.1): `Shape` (count, offsets, values, 5 modes), `Pts` (257-sample cycle, 11 layer phase pairs, `decim`; also a PAD slot's per-voice state), `cycle` (points with span-fraction offsets, natural spline over all points + phantom (0,0), step/linear/spline per segment, mean = sum/256), `play` (k sub-samples: layers summed, drive, clip, × gain, `Decim::filter`, mean), `Decim` (k = smallest with k·rate ≥ 64·f, ≤ 8; two 12.83 kHz ladders at k·rate, `set`/`clear`/`filter`), `MAX_LAYERS` 11, `MAX_OVERSAMPLE` 8. Tests: modes, offsets, spline continuity, rebuild rule, sum-then-clip, oversampling rule, filter DC/Nyquist, high/low-note band loss; ignored speed; opt-in `parity` (cycle vs `cycle-ref`, 7 cases < 1e-4).
- See change: add-tf3-import (tasks 3.1–3.3).
