# schema

Published JSON Schemas (draft 2020-12). Structural only; semantic checks live in the crates that read the files.

| File | Purpose |
|---|---|
| `song.schema.json` | `song/1` song source (`project/music/*.song.json`): header, clips (notes `[start, len, pitch, vel]`, wheel/bend points), tracks + master (receives main\|side, devices kind 64–73 with values by name, placements with loop, lanes). Semantics (names/ranges per kind, references, cycles, duplicate lanes) in `crates/song`; checked by `tests/song-schema.test.ts`. See change: add-synth-daw. `about` (song + track: type, description required; style, tempo, engine, similar_to) — add-import-patterns. Receive `channel` main|side|in1|in2, optional `send` 0–4. Now `song/2` (no `trailer`). See change: unify-v2-algorithms 6.5–6.8 (the format switch). |
| `us-patch.schema.json` | `us-patch/1` preset (`project/music/patches/*.patch.json`): name, kind, non-default values by name, samples. See change: add-synth-daw. Now `us-patch/2`. See change: unify-v2-algorithms 6.5–6.8 (the format switch). |
