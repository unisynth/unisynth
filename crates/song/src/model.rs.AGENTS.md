# crates/song/src/model.rs (sidecar: full detail of the AGENTS.md row)

Pull-only (not auto-injected).

## Types (serde, all `deny_unknown_fields`)
- `FORMAT` "song/2", `FORMAT_V1` "song/1" (migrated on `parse`), `TICKS_PER_BEAT` 960.
- `Song { format, meta?, about?, rate, tempo f64, signature, ticks_per_beat, length ticks, clips, tracks, master }`.
- `Meta { title?, author, description, copyright, license?, url, adapted_from? }`: written right after `format`, skipped when absent; not in the blob.
- `About { type, style?, tempo?, engine?, similar_to, description }`: `Song.about` (after `meta`) and `Track.about` (after `name`); not in the blob.
- `Clip { id, length, notes, wheel?, bend? }` (envelopes → `ctl.wheel` / `ctl.bend` on export); `Note` = `[start, len, pitch, vel]`; `Point` = `[tick, value]` (f64 tick, whole ticks serialised as integers).
- `Track { id, name, about?, color?, volume (linear), mute, receives, devices, placements, lanes }`.
- `Receive { from, channel, send, volume }`: `send` 0 (omitted) = main output, 1–4 extra output pair. `Channel::{Main, Side, In1, In2}`; `Channel::to/of` ↔ blob receive byte 0/2/4/6.
- `Device { kind, values, samples?, speech?, bypass?, preset?, trailer? }`: `speech` = kind 64 Ronan texts; `trailer` is a song/1 field only (an error in song/2).
- `Placement { clip, start, length (null = clip length), loop }`; `Lane { device, param, points }`.
- `Values`: ordered name → f32 map; duplicate key rejected; `get/set/remove/iter/len`. Names are read as written (renames happen in `crate::migrate`).

See change: add-import-patterns, add-v2-import, unify-v2-algorithms.
