# src/law.rs (sidecar: full detail of the AGENTS.md row)

Pull-only (not auto-injected).

- Normalized → physical laws, all clamp `p` (NaN → 0): `cutoff` 20+19 980p², `gain` 16p², `lfo_rate` 0.01·10^(4p), `time` 10p⁴, `transpose` ±24, `coarse` ±24 whole semitones, `fine` ±1, `octave` ±4, `ratio`/`ratio_fine` (Falcon), `fixed_hz`, `feedback` 8p², `pm_out` 4p², `pan` equal power, `env_ms` p²·5000+1, `choice`, `count`, `index`, `scale_amount` (Tunefish), `offset_amount` signed square.
- `host` (host-side): inverses `inv_*`.
- Wavetable laws: `wt_detune` 10p³ Hz, `wt_spread` 10p⁴ Hz, `wt_drive` 1+32p.
- Start phase: `start_spread` 10^(6p−2) samples, `start_phase(u, p, inc)` = `u` scaled to that many samples of the period (whole turn at p = 1);
- `host::inv_start_spread`.
- `bend_semitones`: |bend − 0.5| ≤ 0.5/255 is exactly 0 (automation byte 128 must not detune; test `bend_centre_tests`).
