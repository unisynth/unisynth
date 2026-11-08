# tests

Cross-package tests and shared golden data. Run with `bun test`.

| File | Purpose |
|---|---|
| `song-schema.test.ts` | ajv: every top-level `project/music/*.song.json`, `project/music/patches/*.patch.json` and the `meta` block validate against `schema/song.schema.json` / `schema/us-patch.schema.json`; unknown field, bad note, `song/1` format, missing/extra `meta` keys, non-http url rejected; a song with `adapted_from` must say "adapted" in its description. `released/` not covered. See change: add-synth-daw, unify-v2-algorithms 6.5–6.8. |
