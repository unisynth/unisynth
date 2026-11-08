# src/v2fx.rs (sidecar: full detail of the AGENTS.md row)

Pull-only (not auto-injected).

- Shared V2 effect bodies: `Comp`/`CompP` (peak/RMS, 5700-frame lookahead, 8192 RMS), `Boost`/`BoostP` (150 Hz shelf), `ModDel`/`ModDelP` (chorus/flanger + global delay; left tap idx/idx+1, right one earlier).
- Used by `fx/v2.rs` and `chan.rs`.

## History

- See change: add-v2-import
