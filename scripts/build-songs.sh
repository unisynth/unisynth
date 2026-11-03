#!/bin/bash
# Rebuild site/songs (the listening page: every converted song, played live in the browser by the synth) from the 64k
# working tree (until the sources move into this repo). The page loads the GM bank from ../daw/data/bank.dls, the one
# published with the DAW, so the 17 songs with GM.DLS samplers need site/daw/data/bank.dls (see NOTICE.md).
set -euo pipefail
SRC=${SRC:-$HOME/Project/64k}
OUT=$(cd "$(dirname "$0")/.." && pwd)/site/songs
( cd "$SRC" && cargo build -q --release -p song && bun tools/song-page/build.ts )
test -f "$(dirname "$OUT")/daw/data/bank.dls" || echo "warning: site/daw/data/bank.dls is missing: the GM-bank songs will not play" >&2
rm -rf "$OUT" && mkdir -p "$OUT"
cp -R "$SRC/tools/song-page/out/index.html" "$SRC/tools/song-page/out/synth.wasm" "$SRC/tools/song-page/out/songs" "$OUT/"
echo "site/songs rebuilt from $SRC: $(ls "$OUT/songs" | wc -l | tr -d ' ') songs, $(du -sh "$OUT" | cut -f1)"
