# size-compare (sidecar: full detail of the AGENTS.md rows)

Pull-only (not auto-injected).

## Behaviour
- Legacy vs unified packed size for the seven reference songs × six compile configurations: `z s 3` (precompiled core, `panic=abort`) and `bz bs b3` (`-Zbuild-std=core,compiler_builtins` with `compiler-builtins-mem`, `panic=immediate-abort`; `bz`/`bs` add `optimize_for_size` = the compo build). Tags `+mf` (`-Zmerge-functions=aliases`), `+nomir` (`-Zinline-mir=no`).
- `compare.sh [variant …]` drives `ws-size-probe` (`WS_SONG`), `tf-size-probe` (`TF_SONG`) and `us-size-probe` with exactly each song's devices/features (`synth-convert` example `legacy_features`; `tfmap` maps unified variants `us-fx-distortion-power`, `-eq-3band`, `-delay-ms`, `-flanger-ring`, `-reverb-compact`, chorus, formant to `tf-fx-*`).
- Unified rows build the song as Rust (probe feature `baked`, what ships). Turtles (`ES4_SONGS`): legacy = `tfsynth` on the `tf4m-tfm` output, unified = `tf4m`.
- Resumable: `out/results.csv` rows already present are skipped. `sq()` retries a hung squishy once after 300 s (`timeout`). Report: `tools/unisynth-parity/size-compare.md`.
- `out/results-v1.csv` = the run before shrink-unified-synth (cited for `bz+mf` / `bz+nomir`: byte-identical probes).
- Env: `WS_SONG_DIR`, `TF_SONGS`, `ES4_SONGS`, `TOOLS`, `WINE64`, `WINEPREFIX`; nightly + `i686-pc-windows-msvc`, 64-bit Wine, squishy 0.1.3.

## History
See change: shrink-unified-synth.
