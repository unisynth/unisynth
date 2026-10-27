# size-compare

Packed size of the legacy engines against the unified synth for the seven reference songs, each built under six compile
configurations (opt-level z/s/3, precompiled core or `-Zbuild-std` core). `./compare.sh [variant …]` drives the three
probes (`../ws-size-probe`, `../tf-size-probe`, `../us-size-probe`; the first two take the real song through `WS_SONG` /
`TF_SONG`), appends to `out/results.csv` (finished rows are skipped, so a run resumes) and writes
`tools/unisynth-parity/size-compare.md`. Needs `WS_SONG_DIR`, `TF_SONGS`, `ES4_SONGS` (Enigma-Studio-4 `demos/zoom`, for turtles: legacy is `tfsynth` playing the `tf4m-tfm` output), nightly + `i686-pc-windows-msvc`, 64-bit Wine and
squishy 0.1.3 as in `../us-size-probe`.
