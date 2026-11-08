# src/fx/wrap.rs (sidecar: full detail of the AGENTS.md row)

Pull-only (not auto-injected).

- Distortion mode 3 `wrap` (mini-cauldron 8-bit wrap-around): drive `p·4`, offset `2p − 1`, `((x·drive + offset + 1) mod 2) − 1`.
- Feature `us-fx-distortion-wrap`, module 55.
- `quiet_after` 0 only without offset (an offset plays DC on silence).

## History

- See change: add-cauldron-xm-import, add-quiet-track-skip
