# src/device.rs (sidecar: full detail of the AGENTS.md row)

Pull-only (not auto-injected).

- `UDevice` enum (`Instrument(UInstrument)`, `Effect(UFx)`);
- `Kinds` registry (`need`, `load`; kind 64 instrument, 65–73 effects, else `UnsupportedKind`) + `Device` forwarding (`run`, `run_ext` only with `us-strip`, `quiet_skip` = `us-quiet`, `quiet_after`, `wake`).
- `automation()` → `Err(module::AUTOMATION)` without `us-automation`.
- Feature `threads`: `RENDER_RNG = false`.

## History

- See change: add-synth-threads, shrink-unified-synth, add-v2-import, add-quiet-track-skip
