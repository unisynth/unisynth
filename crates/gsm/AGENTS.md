# crates/gsm

no_std GSM 06.10 full-rate codec (WAV49 / Microsoft framing): decoder + encoder, both bit-exact ports of libgsm 1.0.24, plus the WaveSabre sample-chunk parser/header writer. Shared by `wavesabre` and `unisynth`. Depends on `synth-core` only (`LoadError`, `Pool`). Toolchain: `PATH="$HOME/.cargo/bin:$PATH"`. Checks: `cargo check -p gsm --target i686-pc-windows-msvc -Zbuild-std=core`, `--target wasm32-unknown-unknown`. Long rows have pull-only sidecars `<file>.AGENTS.md`.

| File | Purpose |
|---|---|
| `Cargo.toml` | Package `gsm` (version/edition/license from workspace); description "no_std GSM 06.10 (WAV49) decoder + encoder, port of libgsm 1.0.24, plus the WaveSabre sample-chunk parser". Sole dep `synth-core` (workspace). No features. In root `default-members`. |
| `src/encode.rs` | `GsmEncoder` (`new`, `encode_block(&[i16;320], &mut [u8;65])`) + `encode(&[f32], &mut [u8]) -> usize` + `blocks_for(n)` = ceil(n/320); re-exported at crate root. libgsm encoder port, WAV49 pack, integer path. → see `src/encode.rs.AGENTS.md`. |
| `src/lib.rs` | `GsmDecoder` (`new`, `decode_block`) + `decode(data, &mut [f32])`; `BLOCK_BYTES` 65, `BLOCK_SAMPLES` 320; `GsmError`; `GsmChunk` (`parse`, `pool_need`, `decode`) + `CHUNK_HEADER` 28 + `write_chunk_header`; libgsm COPYRIGHT in module doc; tests vs libgsm vectors. → see `src/lib.rs.AGENTS.md`. |
| `testdata/enc_in.pcm` | Encoder input: 80423 s16le mono samples (silence, sweep, noise, quiet noise, pluck, full-scale squares, Nyquist, DC sine, 103-sample tail). Made by `gen_enc_vectors.py`. |
| `testdata/enc_ref.gsm` | libgsm 1.0.24 `-DWAV49` output for `enc_in.pcm`: 252 × 65-byte blocks, zero-padded tail. Made with external tool `enc49` (`enc49.c`: s16le stdin → blocks stdout; not in repo). |
| `testdata/gen_enc_vectors.py` | Deterministic generator (fixed LCG) for `enc_in.pcm`; usage + regeneration commands in its docstring. |
| `testdata/gsm_vec.c` | libgsm generator: synth signal (sine+LCG noise, silence, square) → WAV49 encode → libgsm decode; writes the two `gsm_vectors.*` files. Rebuild cmd in line 1. |
| `testdata/gsm_vectors.gsm` | 16 × 65-byte WAV49 blocks (libgsm encoder output). |
| `testdata/gsm_vectors.pcm` | libgsm-decoded i16 LE PCM (5120 samples) = exact-match reference; also used by `wavesabre` and `unisynth` tests via `../../gsm/testdata`. |
