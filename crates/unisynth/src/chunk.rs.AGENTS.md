# src/chunk.rs (sidecar: full detail of the AGENTS.md row)

Pull-only (not auto-injected).

- Format: `u8 version` (3, or 4 = v3 + `u16 len≥1` speech text), `count n`, `n × {index, f32}` strictly increasing indices
  (values that differ from `DEFAULTS`), `u8 s`, `s × {u32 size, GSM chunk}`, nothing after. Versions 1/2/unknown →
  `LoadError::UnknownVersion` (`migrate` converts old chunks on the host). A chunk without text is written as version 3.
- Escapes (design D4): count byte `c<0xFF` is `n`, `0xFF` + u16 `n≥255`; index byte `b<0xF0` is the index, `0xF0+k`,`x` →
  `240+256k+x` (`ESC`, `SHORT`, `MAX_INDEX`, `COUNT_ESC`); an escaped count <255 is `BadChunk`. Escapes need `us-chunk-wide`
  (else `UnsupportedModule(CHUNK_WIDE)`); without it `pairs()` steps 5-byte pairs. `Chunk::wide()` = chunk uses an escape.
- API: `Chunk::parse` (finite values 0..=1, raw sampler indices 0..=65535 via `MAX_SAMPLE_INDEX`; ≤ `MAX_SAMPLES` = 255
  entries; a GSM sampler slot's index must exist), `values()` (`PLAYER_DEFAULTS` + pairs), `pairs()`, `sample_count`,
  `sample_bytes`, `sample` (`us-sample-gsm`), `text()` (speech; `us-speech` else `UnsupportedModule(SPEECH)`),
  `sample_index(v)` = original `(int)v` 1-based. Host (`std`/test): `encode`, `encode_text`, `encode_pairs(version, values)`,
  `push_index`, `push_count`. Errors `BadChunk`; a sample section without `us-sample-gsm` → `UnsupportedModule`.
- `us-trusted`: `parse` only locates pairs and samples (no checks; generated songs); walks escapes with `us-chunk-wide`.
- `PLAYER_DEFAULTS` = `DEFAULTS` with indices ≥ 240 zeroed in a build without `us-chunk-wide`/`us-matrix-add`/`us-strip`
  (nothing reads them; size gate).
- Measured: +512 B raw / ~270 B xz for non-V2 songs (escape support).
- History: Doubt-reviewed (cross-model, add-unified-synth task 1.4). See change: add-unified-synth, shrink-unified-synth,
  add-v2-import, unify-v2-algorithms 6.5–6.8.

## Former AGENTS.md row (moved, full text)

- Instrument chunk v3 (v4 = + speech text): `Chunk::parse` validates once (sorted index/f32 pairs, ≤ 255 GSM samples);
- `values()`, `pairs()`, `sample*`, `text()`, `wide()`; host `encode*`; escapes (index ≥ 240, count ≥ 255) need `us-chunk-wide`; versions 1/2 → `UnknownVersion`.
