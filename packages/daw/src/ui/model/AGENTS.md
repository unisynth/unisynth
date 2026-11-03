# packages/daw/src/ui/model

Pure model helpers (no DOM, no store writes except where noted): parameter tables and formatting, track/device helpers, base64 and GM helpers.

| File | Purpose |
|---|---|
| `bytes.ts` | `b64ToBytes`, `bytesToB64` (32 KB chunks), `gmInstrument(idx)` (1-based GM.DLS index: drums from 16385, bank×128+prog). daw-solid-ts 9.1 step 1. |
| `params.test.ts` | Unit tests of params/track/bytes on the real engine tables (instantiates `synth.wasm` from `buildSynthWasm()`, `song_tables`/`song_physical`): choice, ip/optName/oscType/slotUsed, effect modes, fmtVal/panText, envelopes, engineDev/patOf/cleanDev, base64/GM. |
| `params.ts` | Device parameters from the engine tables (daw-solid-ts 9.1 step 1): `initParams(tables, physical)` once; `SLOTS`, `choice`, `nModes`, `modeOf`/`tableOf` (raw reads), `paramOf`, `devName`, `autoParams`, `effectChoices`, `INST`, `ip` (name map built at init), `vget`, `optName`, `envRate`, `oscType`, `smpShapes`, `slotKeys`, `TARGETS`, `modSrcName`, `routesOf`, `slotUsed`, `envMs`, `tfEnvSeconds`, `envCurve`, `panText`, `fmtVal` (proxy reads: knob text tracks law/sustain/slope/osc type). `optValue(name, label)` (stored value of an option); `oscType` over the 13 v3 kinds (5–11 naive, 12 clocked). See change: unify-v2-algorithms 6.5–6.8 (the format switch). 9.4: `slotKeys` cached per instrument table (returned arrays shared: do not mutate). |
| `track.ts` | Track helpers (daw-solid-ts 9.1 step 1): `instOf`, `fxOf`, `devicesOf`, `links`, `engineDev` (bypassed left out), `cleanDev` (drops `_edited`/`_dev`; `UIDevice`), `laneText`, `patOf` (creates a missing pattern: a model write), `carry`. |
