# us-size-probe

Standalone (`[workspace]`) i686 `no_std` probe that measures the code size of the unified synth (`crates/synth-core` +
`crates/unisynth`). `build.rs` embeds the unified song named by `US_SONG`; `start` renders it to 16-bit PCM and exits
with a checksum, so every enabled module is reachable. No audio output.

`./measure.sh` builds the all-modules probe (converted WaveSabre `showcase` song) and one probe per reference song with
exactly the features `synth-convert features` lists for it, and writes raw, `xz -9e` and squishy sizes (squishy via
64-bit Wine when installed) to `tools/unisynth-parity/size.md`, which the parity report includes. Same notes as
`../ws-size-probe`: `_fltused`, 64-bit division forwarders, an MSVC-contract `__chkstk`.
