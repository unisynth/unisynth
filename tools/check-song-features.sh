#!/usr/bin/env bash
# Compo-build check of a unified song: takes the features `synth-convert features` lists, then
#   1. renders the song on the host with `unisynth` built with exactly those features,
#   2. checks the i686 core-only player target (no std) builds with exactly those features,
#   3. with --minimal: drops each feature in turn and requires the song to fail to load or to sound different.
# Usage: tools/check-song-features.sh <unified.song> [--minimal] [bank.dls]
set -uo pipefail
cd "$(dirname "$0")/.."
export PATH="$HOME/.cargo/bin:$PATH"
song=${1:?usage: check-song-features.sh <unified.song> [--minimal] [bank.dls]}; shift
minimal=0; bank=""
for a in "$@"; do [ "$a" = "--minimal" ] && minimal=1 || bank=$a; done
tmp=$(mktemp -d); trap 'rm -rf "$tmp"' EXIT
feats=$(cargo run -q -p synth-convert -- features "$song") || { echo "FAIL  features"; exit 1; }
echo "features: ${feats:-(none)}"
render() { # features, out.wav
  cargo run -q --release -p unisynth --no-default-features --features "std${1:+,$1}" --example render -- "$song" "$2" $bank
}
render "$feats" "$tmp/full.wav" || { echo "FAIL  host render with the listed features"; exit 1; }
echo "ok    host render"
cargo check -q -p unisynth --no-default-features ${feats:+--features "$feats"} --target i686-pc-windows-msvc -Zbuild-std=core \
  || { echo "FAIL  i686 build with the listed features"; exit 1; }
echo "ok    i686 core-only build"
[ $minimal = 1 ] || exit 0
fail=0
IFS=, read -ra list <<<"$feats"
for f in "${list[@]}"; do
  [ "$f" = us-osc-sample ] && continue # implied by the sample sources
  [ "$f" = us-matrix ] && [[ ",$feats," == *",us-matrix-add,"* ]] && continue # implied by us-matrix-add
  [ "$f" = us-chunk-wide ] && [[ ",$feats," == *",us-strip,"* ]] && continue # implied by us-strip
  rest=$(printf '%s\n' "${list[@]}" | grep -vx -- "$f" | paste -sd, -)
  if render "$rest" "$tmp/less.wav" 2>/dev/null && cmp -s "$tmp/full.wav" "$tmp/less.wav"; then
    echo "FAIL  $f is listed but the song plays the same without it"; fail=1
  else
    echo "ok    $f is needed"
  fi
done
exit $fail
