# crates/song/src/validate.rs (sidecar: full detail of the AGENTS.md row)

Pull-only (not auto-injected).

`validate(song)` → every `Issue { path (JSON pointer), message }`; `seg` escapes pointer segments. Export refuses a song with any issue.

## Checks
- Header: `format` = song/2, rate 8000..192000, tempo 20..999 finite, signature, `ticks_per_beat` 960, length range (sample positions fit the blob's i32).
- `/meta/*`: author, description, copyright, url not blank; url http(s); `adapted_from` not blank and the description must contain "adapted". `/about/*`, `/tracks/i/about/*`: type, description, optional text and `similar_to` entries not blank.
- Clips: ids unique non-empty, length ≥ 1, notes (pitch 0..127, velocity 1..127, length ≥ 1, start inside the clip). Track ids unique non-empty.
- Receives: unknown track, from master ("the master cannot send; route through a bus before the master"), self, `send` > `OUT_PAIRS`, volume finite ≥ 0, cycles.
- Devices: legacy kind → synth-convert, unknown kind; unknown parameter at `/…/values/<name>`; range (raw whole 0..65535); samples (instrument only, ≤ `MAX_SAMPLES`, base64); `speech` instrument only, ≤ 65 535 bytes joined; `trailer` is an error in song/2.
- Track volume finite ≥ 0. Placements: unknown clip, start ≥ 0, length ≥ 1, ends after the song.
- Lanes: device exists, param known and not structural, duplicate ("parameter already automated on this track"), points finite, value 0..1, in order, inside the song. Clip wheel/bend vs a track lane on the same `ctl.*` parameter = conflict; missing `ctl.*` parameter or no instrument = error.

See change: add-v2-import, unify-v2-algorithms.
