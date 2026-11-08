# src/needs.rs (sidecar: full detail of the AGENTS.md row)

Pull-only (not auto-injected).

- Host (`std`): modules a song needs.
- `instrument(chunk, reach)` = key set of `instrument_why` (modules with the parameters that call for each, `Why`); mirrors `check_modules`/`check_voice`/`check_v2`/`Kind::all`/`smp::pool_need` over the span each automated parameter reaches (a lane on a mod source/target reads every envelope and reaches noise; a routing or Add lane needs `MATRIX`/`MATRIX_ADD`;
- `CHUNK_WIDE` when `Chunk::wide()`;
- `SPEECH` with text;
- `STRIP` for the channel strip or an aux oscillator; clocked → `OSC_CLOCKED`; naive types, combine, filter forms, env law/curve/slopes, LFO and voice settings, quirks).
- DLS wave and instrument sources need `us-sample-dls`.
- `effect(kind, chunk)` → `fx::module_of`;
- `feature(id)` → `module::feature`.

## History

- See change: add-unified-synth, shrink-unified-synth, add-v2-import, add-cauldron-xm-import, unify-v2-algorithms 6.5–6.8
