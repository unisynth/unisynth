# crates/song/tests/song.rs (sidecar: full detail of the AGENTS.md row)

Pull-only (not auto-injected).

Spec scenarios of song-source, one test per scenario:
- Export: minimal song, tick 960 → 22 050 at 120 BPM, linked placements, same-sample order (offs first, pitch order), overlapping same-pitch, loop flattening, deterministic, mute, effect-only bus, sidechain ch 2, extra-pair receives (`receives_on_extra_pairs_round_trip`, `a_send_past_the_extra_pairs_is_an_error`), automation ticks/bytes.
- Validation: paths for unknown/bad parameter (dotted names stay one segment), duplicate lane, master receive, every problem with a path, cycle, unknown fields / bad format, export refuses an invalid song.
- Blob round trip (chunks/routing/volumes/automation identical, notes ≤ ½ tick), automation after the song end cut not stretching, note-on before note-off at one sample keeps its order, released-song quirks import as valid songs, legacy kinds → synth-convert.
- Presets (round trip same chunk, defaults dropped), cutoff 0.5 → 5015 Hz, effect tables vs engine parameter counts, every oscillator kind and engine effect mode named, controllers semitone range, wavetable spread, fractional ticks exact.
- Metadata and `about`: round trip, not in the blob, validation paths, adapted-work rule; `apply_meta` names + descriptions, count mismatch, unknown field.
- `no_source_names_in_the_parameter_table` (tables, options, modes, modules, features).
- Migration: `an_old_song_file_migrates_once_and_a_new_one_never` (song/1 old names → song/2; a song/2 file with an old name is an error), `old_effect_mode_lanes_and_quirk_reaching_lanes_migrate`.

See change: unify-v2-algorithms, add-import-patterns, add-v2-import.
