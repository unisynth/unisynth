# src/model.ts (sidecar: full detail of the AGENTS.md row)

Pull-only (not auto-injected).

## API
- Types: `Song` (`song/2`), `SongTrack`, `Device` (`speech`, `samples`, `bypass`, `preset`), `LanePoint` `[bar, value, tick?]`, `About`, `UIState`, `UITrack`, `UIClip`, `UINote`, `UIPattern`; `TPB` 960, `ticksPerBar`.
- `toSong(ui, audible?)` / `fromSong(song)`; `cleanDevice`, `prettyJson` (same layout as `crates/song`), `dbToGain`/`gainToDb`.
- `audible(tracks, t)`: live-preview solo — soloed, send source of a soloed track (sidechain feeds stay), or bus a soloed track sends to; mute wins.

## Behaviour
- Editor model: bars (1-based), 1/16 steps, dB volumes, sends stored on the sender, one pattern per clip name per track.
- Exact round trip for unedited songs (checked on every `project/music` song): `meta` kept; `about` kept (song after `meta`, track after `name`); clip ids kept; colours only as loaded; loaded gains while dB unchanged; lane points keep their loaded tick (`LanePoint`); `length0` while the bar count is unchanged; receive order per track (`recv0`); master receives as loaded (`mrecv0`, `ids0`) + editor-made tracks.
- Receives keep `channel` in1/in2 and `send` (sends tuple 5th element = sending pair).
- `cleanDevice` keeps `Device.speech` (speech texts of a speech-enabled instrument).
- Non-flat wheel/bend envelopes are written (validation then asks for `ctl.*`); flat ones are not.

## History
- See change: add-synth-daw, add-import-patterns (`about`), unify-v2-algorithms 6.5–6.8 (the format switch: `song/2`).
