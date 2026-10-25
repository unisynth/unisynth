# tf-size-probe

Standalone (`[workspace]`) i686 `no_std` probe for the code size of `crates/tfsynth` (+ `synth-core`): `build.rs`
writes a 13-instrument `.tfm` from `crates/tfsynth/tests/patches.rs` (6 instrument patches + one per effect kind);
`start` plays it with `TfmPlayer` to 16-bit PCM and exits with a checksum. No audio output.

`./measure.sh` → raw, `xz -9e` and squishy 0.1.3 sizes for `--features all` (every effect kind) and the default
build (no effects: slots act as none). Same `_fltused` / 64-bit division / `__chkstk` support as `ws-size-probe`.
Not run on i686 yet (macOS Wine is 64-bit only).
