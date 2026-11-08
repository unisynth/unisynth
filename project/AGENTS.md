# project

Song library (LLM-editable). Music: `music/*.song.json` (`schema/song.schema.json`, `schema/us-patch.schema.json`; validation in `crates/song`). `music/released/` = converted songs from released intros / exe music (see `NOTICE.md`).

| File | Purpose |
|---|---|
| `music/demo.song.json` | Example `song/1` (128 BPM, 8 bars): Kick, Bass (Crusher, cutoff lane), Lead (Echo), Pad, Verb bus (Cathedral) ← Lead/Pad, Master (Compressor). Validated/rendered by `crates/song/tests/demo.rs`, schema-checked by `tests/song-schema.test.ts`. See change: add-synth-daw. |
| `music/NOTICE.md` | Attribution and licences of the six demo songs (adapted works): original, author as stated upstream, copyright, MIT / GPL-3.0-or-later with the MIT text. |
| `music/patches/*.patch.json` | `us-patch/1` presets `acid-bass`, `glass-pad` (kind 64, non-default values by name). |
| `music/{global-groove,elysian,engage,eladamri,no-remorse,tf4testsong}.song.json` | The reference songs the A/B renders play, converted by `tools/demo-songs` (WaveSabre test projects, MIT; Tunefish modules, GPL-3.0-or-later). Each has `meta` (author, description, copyright, license, url, `adapted_from`); the description says it is an adapted version. Validated/exported by `crates/song/tests/demo.rs`. See change: add-unified-synth. |
