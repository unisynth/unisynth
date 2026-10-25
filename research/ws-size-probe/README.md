# ws-size-probe

Standalone (`[workspace]`) i686 `no_std` probe that measures the code size of the WaveSabre Rust port
(`crates/synth-core` + `crates/wavesabre`). `build.rs` writes a synthetic song from
`crates/wavesabre/tests/songs.rs` (`showcase()` for `--features all`, `slaughter_automation` for `--features slaughter`);
`start` renders it to 16-bit PCM and exits with a checksum, so everything is reachable. No audio output.

`./measure.sh` → raw, `xz -9e` and squishy 0.1.3 sizes (squishy via 64-bit Wine when installed).

Notes: provides `_fltused`, `__aulldiv`/`__aullrem`/`__alldiv`/`__allrem` forwarders and an MSVC-contract `__chkstk`
(the `Player` frame exceeds 4 KB). The probe has not run on i686 yet (macOS Wine is 64-bit only; Docker was off).
