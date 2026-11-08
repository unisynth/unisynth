# src/v2m.rs (sidecar: full detail of the AGENTS.md row)

Pull-only (not auto-injected). Farbrausch V2 `.v2m` reader and sequencer. Test writer in `v2m/write.rs`. See change: add-v2-import.

- `canonicalize(bytes)` → `(version 0–6, v6 bytes)` byte-identical to V2 `ConvertV2M`; quirks kept: patch count `offsets[0]/4`, growing-bound mod-dest remap, 4 zero tail bytes; version from globals size + patch sizes.
- `parse` → `V2m{version, bytes, timediv, maxtime, globals[23], patches: Patch{params[89], mods (src, amount, dest)}, speech texts}`.
- `V2m::timeline(rate)` → `Timeline{events: Event{frame, chan, Msg::{Note (vel 0 = off), Program, Ctl 1–7, Bend{lo,hi}}}, end_frame}`: replays `V2MPlayer` Tick + `UpdateSampleDelta` exactly (per-channel order pc, cc1–7, bend, notes; first tick at frame 0; remainder carries only on u32 overflow; bend planes read at the program-change width).
- Tables `PATCH_PARAM_VER`, `GLOBAL_PARAM_VER`, `PATCH_DEFAULTS`, `GLOBAL_DEFAULTS`; `PATCH_PARAMS` 89, `GLOBAL_PARAMS` 23, `MAX_VERSION` 6; `patch_param_count`, `global_size`, `patch_size`.
- Errors name the problem (`ConvError::Input`, "v2m: …").
