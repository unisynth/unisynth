#!/bin/bash
# Build the static DAW and publish it to https://preview.judo.technology/dsp/ (shared bucket
# gs://preview-judo-technology, prefix dsp/, project judo-ng, shared LB chain "preview" — same as deltadot/ and basa/).
# UNLISTED, NOT PRIVATE: anyone with the URL can read it, including the GM.DLS bank and the released-intro songs
# (third-party, no licence). For the owner's own testing only; remove with: gcloud storage rm -r gs://preview-judo-technology/dsp
set -euo pipefail
cd "$(dirname "$0")/../.."
G=${GCLOUD:-$(command -v gcloud || echo ~/Project/google-cloud-sdk/bin/gcloud)}
export GM_DLS=${GM_DLS:-$HOME/Downloads/64k-tools/synth-lab/ws/gm.dls}
OUT=packages/daw/build/dsp
bun packages/daw/build-static.ts "$OUT"
DST=gs://preview-judo-technology/dsp
$G storage rsync "$OUT" "$DST" --recursive --delete-unmatched-destination-objects --cache-control 'public, max-age=300'
# types gcloud may guess wrong: WebAssembly.compileStreaming needs application/wasm
$G storage objects update "$DST/synth.wasm" --content-type application/wasm
$G storage objects update "$DST/synth-mt.wasm" --content-type application/wasm
# Threaded bounce (change add-wasm-threads) needs cross-origin isolation, i.e. the page served with
#   Cross-Origin-Opener-Policy: same-origin   and   Cross-Origin-Embedder-Policy: require-corp
# (+ Cross-Origin-Resource-Policy: same-origin on the files). GCS objects cannot carry these; they would be custom
# response headers on the LB's backend bucket, which this bucket shares with deltadot/ and basa/ — so they are not set
# here. Without them the preview is not isolated and the DAW bounces single-threaded (same output).
$G storage objects update "$DST/data/bank.dls" --content-type application/octet-stream
echo "published: https://preview.judo.technology/dsp/ (CDN cache up to 5 min)"
