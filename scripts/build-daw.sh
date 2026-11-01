#!/bin/bash
# Rebuild site/daw from the 64k working tree (until the sources move into this repo).
# GM_DLS: the Windows GM bank to publish as site/daw/data/bank.dls (owner's decision, see NOTICE.md).
# Always set explicitly: if GM_DLS is unset the server would fall back to the OS bank (macOS gs_instruments.dls).
set -euo pipefail
SRC=${SRC:-$HOME/Project/64k}
GM_DLS=${GM_DLS-$HOME/Downloads/64k-tools/synth-lab/ws/gm.dls}
OUT=$(cd "$(dirname "$0")/.." && pwd)/site/daw
( cd "$SRC" && GM_DLS="$GM_DLS" bun packages/daw/build-static.ts "$OUT" )
# Cache-bust: Pages caches every file for 10 min, so a new main.js with a cached old index.html fails to start.
python3 - "$OUT" <<'PY'
import hashlib, re, sys, os
d = sys.argv[1]
h = lambda f: hashlib.sha1(open(os.path.join(d, f), 'rb').read()).hexdigest()[:10]
assets = ["synth.wasm", "synth-mt.wasm", "worklet.js", "bounce-worker.js", "bounce-helper.js"]
m = open(os.path.join(d, "main.js")).read()
for a in assets:
    n = m.count('"%s"' % a); assert n, a
    m = m.replace('"%s"' % a, '"%s?v=%s"' % (a, h(a)))
open(os.path.join(d, "main.js"), "w").write(m)
i = open(os.path.join(d, "index.html")).read()
for a in ["main.js", "style.css"]:
    assert i.count('"%s"' % a) == 1, a
    i = i.replace('"%s"' % a, '"%s?v=%s"' % (a, h(a)))
open(os.path.join(d, "index.html"), "w").write(i)
PY
echo "site/daw rebuilt from $SRC (bank: ${GM_DLS:-none})"
