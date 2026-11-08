# src/fx/comb.rs (sidecar: full detail of the AGENTS.md row)

Pull-only (not auto-injected).

- Flanger mode 2 `comb` (mini-cauldron comb + doubler): 4 params delay K (`round(p·16384)` frames, pool `MAX` 16384 + 2 per channel), amount g, sweep rate `p·0.01`, restart (value change → t = 0).
- Sweep 0: `x[n] − g·x[n−K]`; sweep s: `(x[n−K] + x[n−K+s·t])/2` (linear interpolation, offset ≤ K).
- Feature `us-fx-flanger-comb`, module 56.
- `quiet_after` = line capacity;
- `wake(skipped)` advances sweep counter `t`.

## History

- See change: add-cauldron-xm-import, add-quiet-track-skip
