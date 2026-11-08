# src/fx/curve.rs (sidecar: full detail of the AGENTS.md row)

Pull-only (not auto-injected).

- Distortion mode 1 `curve` (QM Screwdriver, qm210/WaveSabre fork, MIT): 8 params X1, Y1, X2, Y2, LogPower0..2 (`2^(8v−4)`), DryWet; three-segment power curve on `|x|`, sign kept;
- X1/X2 clamped against the current other value (order-dependent, as the original).
- Feature `us-fx-distortion-curve`, module id 38.
