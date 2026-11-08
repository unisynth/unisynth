# src/id3.rs (sidecar: full detail of the AGENTS.md row)

Pull-only (not auto-injected).

- `Id3::new(&[f32;11])`, `set_param(i, v)` (atomic: `Err(Index)` i ≥ 11, `Err(Range)` non-finite / outside [0,1]), `process(l, r)` in place (equal lengths, panics otherwise; aux pair out of scope, note events inert).
- Waveshaper: per sample `S(x·g_in, k)·g_out` with drive `k` (P0), ceiling `c` (P1: `x·k/c`, result `·c`), mode `trunc(P4·3.999)` 0 clip / fold (P2) 1 `×π/2` 2 cubic 3 soft knee (P8 threshold, P9 width, P10 slope), clamp to ±1 (NaN → −1), `trunc(2·P5)` = none / 2-point / 4-point average of the previous and current raw input (**sub-sample paths get no in/out gain**), mix `x + (wet−x)·P3`, `prev ← x` per channel; left block before right.
- All 11 params modelled, nothing in range unsupported (c = 0, w = 0 follow IEEE).
- Pure f32 `+ − × /` (knee `powf(u,2)` = `u·u`).
- **Bit-exact except 36 of 20,042,752 whole-song samples (≤ 2 ulp, 1.19e-7, the C library's `powf(u,2)` is 1 ulp high on rare arguments)**, 13 synthetic vectors all but 5 samples (≤ 6e-8).
- Reference tolerance `1e-7 + 3e-7·|ref|`.
- Mode / averaging selectors: `(v·3.999) as u8`, `(v+v) as u8`.
