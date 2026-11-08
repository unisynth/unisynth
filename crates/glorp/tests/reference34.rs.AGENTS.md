# tests/reference34.rs (sidecar: full detail of the AGENTS.md row)

Pull-only (not auto-injected).

- Env-gated (`GLORP_REF_ROOT`) replay of the 2 active ID 34 instances (song devices 25, 26; 153,600 samples each) from block 0 (silence before the tap window, phase integrates), every recorded `SetParam` applied before its block; tolerance 3e-7 + 3e-7·|ref|; negative control.
- Result: dev 25 bit-exact, dev 26 max abs 1.19e-7 (1 ulp; 361 samples, all from sine-table node 305; patching that node makes both exact).
