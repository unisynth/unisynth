# tests

Cross-package tests and shared golden data.

| File | Purpose |
|---|---|
| `song-schema.test.ts` | every `project/music/*.song.json` (an adapted song's description says so), the `meta` schema, and `project/music/patches/*.patch.json` validate against `schema/song.schema.json` / `schema/us-patch.schema.json` (ajv); unknown field, bad note, wrong format rejected. See change: add-synth-daw. song/1 is refused (migrate it). See change: unify-v2-algorithms 6.5–6.8 (the format switch). |
