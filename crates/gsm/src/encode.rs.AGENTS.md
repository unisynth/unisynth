# src/encode.rs (sidecar: full detail of the AGENTS.md row)

Pull-only (not auto-injected). `no_std`, no heap. Bit-exact port of the libgsm 1.0.24 WAV49 encoder; reference build `-DWAV49`, no `FAST`, no `USE_FLOAT_MUL`, no `LTP_CUT` (integer path).

- Ported sources: `gsm_encode.c` (WAV49 pack: first frame of a pair keeps 32 bytes + 4-bit `frame_chain`, second keeps 33; pack code mechanically generated from the C), `code.c`, `preprocess.c`, `lpc.c`, `short_term.c`, `long_term.c`, `rpe.c`, `add.c` (`norm`, `div`, `mult` non-rounding, `l_add` = `i32::saturating_add`), `table.c` (`DLB`, `H`, `NRFAC`; `QLB`, `FAC` shared from crate root), `private.h`. Reuses root `add/sub/mult_r/asr/asl`.
- API: `GsmEncoder::new()` (all-zero state), `encode_block(&[i16;320], &mut [u8;65])` = two `gsm_encode` calls; `encode(&[f32], &mut [u8]) -> usize` with one fresh encoder (f32 → i16 is `(x·32768) as i16`: trunc, saturating, NaN → 0; last block zero-padded; returns `blocks_for(len)·65`; panics when `out` is shorter); `blocks_for(n)` = `ceil(n/320)`. All re-exported at crate root.
- Quirks matched: `mp` stays i32 (not word); `msp`/`lsp` word-truncated; autocorrelation scales `so` in place and rescales with `<<` (lossy, feeds the filter); `dpp` aliases the `dp` slot in `dp0` (estimate written into the slot being filled); `dp0` holds 280 words, shifted by 160 after each frame.
- Debug build has no overflow panics on extreme input.
- Tests: `encoder_matches_libgsm` etc. live in `lib.rs` (see `lib.rs.AGENTS.md`), vectors in `testdata/enc_in.pcm` / `enc_ref.gsm`.
- History: See change: add-unified-synth.
