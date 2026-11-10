# us-size-probe (sidecar: full detail of the AGENTS.md rows)

Pull-only (not auto-injected).

## Probe
- Standalone (`[workspace]`) i686 `no_std` crate for `synth-core` + `unisynth`. `build.rs` embeds the unified song named by `US_SONG`; `start` renders it to 16-bit PCM in 4096-frame chunks, exits with a rotate-xor checksum. No audio output. Needs nightly + `i686-pc-windows-msvc`.
- Features are `unisynth/us-*` (passed as `--features unisynth/us-osc-wt,…`). `rep` string `memcpy/memmove/memset` and `__CxxFrameHandler3` stand in for the C runtime.
- Feature `baked` (= `unisynth/us-trusted`): `build.rs` writes the song with `synth_convert::rust::generate` (build-dependency); `start` plays `baked::SONG`. What a compo build ships.
- `US_SONG_RS=<file>`: with `baked`, `build.rs` takes a prepared song file instead of generating one.
- `--cfg us_v3_pad`: a 34-float static (the v3 layout's added defaults), read once so it links (unify-v2-algorithms 6.4).
- `target-*/`, `target/`, `out/` ignored.

## Scripts
- `measure.sh`: builds every row with `baked`, compo flags. Rows: all-modules (converted WaveSabre `showcase`, `us-all`) + one per reference song with exactly its `synth-convert features` (turtles from `ES4_SONGS/zoom.tf4m` via `tf4m`, else exported from `project/music/turtles.song.json`). `V2_SONGS` adds `v2-<song>` rows (`V2_LIST`, default fr08 brullwurfel candytron kkrieger). Writes `tools/unisynth-parity/size.md` (raw / xz -9e / squishy 0.1.3 via Wine). Env: `WS_SONG_DIR`, `TF_SONGS`, `ES4_SONGS`, `V2_SONGS`, `TOOLS`, `WINE64`.
- `songs-size.sh [tree]`: per-song compo probe (baked, own feature list) raw / xz / squishy for the seven reference songs + V2 songs converted by that tree. Gate D7.6 of unify-v2-algorithms. `ONLY=<regex>` measures a subset; `EXTRA_FEATS` adds features to every song; `DROP_FEAT` removes one (its cost; a baked build refuses a song missing a module). Atrium and Elevated join when `ELEVATED_SRC`/`ELEVATED_REF` hold them (add-elevated-import).
- `attr.sh <song> [depth]`: no-LTO no-inline build in the compo configuration; linker map summed per module path by `attr.py` (ranks, not totals).
- `marginal.sh [feature…]`: all-modules build against each leaf feature removed → `out/marginal.csv` (feature,raw,xz). Leaves include `us-voice-*`.
- `ablate.sh [switch…]`: per song, the compo build against one optional voice path stubbed out via `--cfg us_ablate="<switch>"` (`a+b` combines; default unison mono glide lfo ctl oneshot spread fine steal; stubs are temporary edits, not committed) → `out/ablate.csv` (song,switch,raw,xz).

## History
See change: shrink-unified-synth, add-v2-import, unify-v2-algorithms.
