# tests/reference.rs (sidecar: full detail of the AGENTS.md row)

Pull-only (not auto-injected).

- Env-gated (`GLORP_REF_ROOT` = `ws-intros/out`; skipped when unset, so plain `cargo test` does not prove parity; malformed `tapspec.txt` rows with end < start are ignored via `checked_sub`) replay of the 10 active ID 38 instances' recorded taps from the Wine-run original: initial chunk from `389811-2.bin`, every recorded `SetParam` applied via `set_param`; tolerance 1e-6 + 4e-7·|ref|, non-distorting windows asserted bit-exact; negative control flips an expected bit.
- Result: 5 distortion-free windows exact, 5 distorted ≤ 2.4e-7.
