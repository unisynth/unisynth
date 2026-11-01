#!/bin/bash
# Rebuild site/daw from the 64k working tree (until the sources move into this repo).
# GM_DLS is set empty on purpose: the GM bank (Windows gm.dls / macOS gs_instruments.dls) is not ours to publish.
set -euo pipefail
SRC=${SRC:-$HOME/Project/64k}
OUT=$(cd "$(dirname "$0")/.." && pwd)/site/daw
( cd "$SRC" && GM_DLS= bun packages/daw/build-static.ts "$OUT" )
test ! -e "$OUT/data/bank.dls" || { echo "bank.dls must not be published" >&2; exit 1; }
echo "site/daw rebuilt from $SRC"
