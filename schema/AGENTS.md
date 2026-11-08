# schema

Published JSON Schemas (draft 2020-12). Structural only; semantic checks live in the crates that read the files.

| File | Purpose |
|---|---|
| `song.schema.json` | `song/2` song source (`project/music/**/*.song.json`). Header (`rate`, `tempo`, `signature`, `ticks_per_beat` 960, `length`), optional `meta` (author, description, copyright, url required; title, license, `adapted_from`) and `about` (song + track: `type`, `description` required; style, tempo, engine, `similar_to`). `clips` (notes `[start, len, pitch, vel]`, `wheel`/`bend` points). `tracks` + `master`: receives (`channel` main\|side\|in1\|in2, optional `send` 0–4), devices kind 64–73 (values by name, `samples`, `speech`, `bypass`, `preset`), placements with `loop`, lanes. No `trailer`; `song/1` rejected. Semantics (names/ranges per kind, references, cycles, duplicate lanes) in `crates/song`; checked by `tests/song-schema.test.ts`. See change: add-synth-daw, add-import-patterns, unify-v2-algorithms 6.5–6.8. |
| `us-patch.schema.json` | `us-patch/2` preset (`project/music/patches/*.patch.json`): `name`, `kind` 64–73, non-default `values` by name, optional `samples`. Names/ranges checked by `crates/song`. See change: add-synth-daw, unify-v2-algorithms 6.5–6.8. |
