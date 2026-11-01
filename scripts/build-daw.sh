#!/bin/bash
# Rebuild site/daw from the 64k working tree (until the sources move into this repo).
# GM_DLS: the Windows GM bank to publish as site/daw/data/bank.dls (owner's decision, see NOTICE.md).
# Always set explicitly: if GM_DLS is unset the server would fall back to the OS bank (macOS gs_instruments.dls).
set -euo pipefail
SRC=${SRC:-$HOME/Project/64k}
GM_DLS=${GM_DLS-$HOME/Downloads/64k-tools/synth-lab/ws/gm.dls}
OUT=$(cd "$(dirname "$0")/.." && pwd)/site/daw
( cd "$SRC" && GM_DLS="$GM_DLS" bun packages/daw/build-static.ts "$OUT" )
echo "site/daw rebuilt from $SRC (bank: ${GM_DLS:-none})"
