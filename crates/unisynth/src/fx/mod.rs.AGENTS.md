# src/fx/mod.rs (sidecar: full detail of the AGENTS.md row)

Pull-only (not auto-injected).

- Effect kinds 65–73: `UFx` (`need`/`new` from chunk `[mode, params…]` + 4-byte `TAG` trailer, `Device` impl; mode is chunk-only;
- `param_count` = 1 + mode params), `Effect` trait (`PARAMS`, `need`, `new`, `set`, `reset`, `run`, `quiet_after`, `wake`), `modes!` table (feature-gated variants), `MODES` (22: kind, mode, module id), `mode_total`, `mode_count` (old-layout count, migration), `mode_of(_value)`, `mode_value`, `module_of`, `is_effect`, host `encode`.
- Mode = plain `Choice(mode_total)`.
- Missing mode → `UnsupportedModule(FX_*)`; no `TAG` → `UnknownVersion`, partial value → `BadChunk` (not checked in `us-trusted`).

## History

- See change: add-unified-synth, add-v2-import, add-cauldron-xm-import, add-quiet-track-skip, unify-v2-algorithms 6.5–6.8
