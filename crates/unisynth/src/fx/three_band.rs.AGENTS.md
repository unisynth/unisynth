# src/fx/three_band.rs (sidecar: full detail of the AGENTS.md row)

Pull-only (not auto-injected).

- EQ mode 1 `3band` (Tunefish): corners 880 Hz / 5 kHz computed for the song rate (original fixed 44.1 kHz; identical at 44.1 kHz).
- 4 params: 3 gains + `pan` (0 off, byte 1–255 equal-power hard left→right after the bands).
- Gain/pan changes ramp linearly over their block (`cur`), snap on first block/reset.
- `quiet_after` 0;
- `wake` restarts the ramp from the parameters set while asleep (`targets`).

## History

- See change: add-cauldron-xm-import, add-quiet-track-skip
