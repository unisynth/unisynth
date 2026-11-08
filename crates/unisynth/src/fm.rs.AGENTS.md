# src/fm.rs (sidecar: full detail of the AGENTS.md row)

Pull-only (not auto-injected).

- Elementary functions in place of `libm` (8–9 KB of the i686 player): f64 `exp2`/`log2`/`pow` (exponent split + polynomial;
- `pow` handles `x<0` with whole `y`, `0^y`), `sin_cos`/`sin`/`cos` (π/2 in three parts, exact for `|x|<1e6`, finite 0/1 beyond), f32 wrappers `expf exp2f powf log10f sinf cosf sincosf`, `sqrt`/`sqrtf` hardware on x86 (SSE2) else `libm`.
- Tests against `libm` (rel < 2e-15 f64, ≤ 2 ulp f32).
- Other code still calls `libm` for `floor`, `fabsf`, `rint`, `atan`, `tan`, `fmodf`.

## History

- See change: shrink-unified-synth
