#!/usr/bin/env bash
# Builds the Tunefish 3 reference renderer for tools/tf3-parity OUTSIDE the repo (the Enigma Studio 3 source states no
# licence: never committed). Copies the synth sources next to our MIT shim (shim/) so their "../system" includes resolve
# to it, and links them with our harness render-ref.cpp.
#   TF3_REF=<dir> (default /tmp/tf3-ref): Enigma-Studio-3 checkout; cloned at the pinned commit if absent.
#   TF3_OFFLINE=1: do not clone; exit 0 with a message when the checkout is absent.
# Output: $TF3_REF/build/render-ref-player (the intro's ePLAYER build), $TF3_REF/build/render-ref-editor and
# $TF3_REF/build/cycle-ref (one point-oscillator cycle) and $TF3_REF/build/pad-ref (one PAD table), for crates/unisynth's
# opt-in cycle and table parity.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
REF="${TF3_REF:-/tmp/tf3-ref}"
COMMIT=85d65b610e2cd92fe435bd28650260056ed121f4
if [ ! -d "$REF/code/eshared/synth" ]; then
  if [ -n "${TF3_OFFLINE:-}" ]; then
    echo "tf3 build-ref: skipped (no Enigma-Studio-3 checkout at $REF)"
    exit 0
  fi
  git clone -q https://github.com/enigmastudio/Enigma-Studio-3 "$REF"
  git -C "$REF" checkout -q "$COMMIT"
fi
OUT="$REF/build"
TREE="$OUT/tree"
rm -rf "$TREE"; mkdir -p "$TREE/synth"
cp "$REF"/code/eshared/synth/*.cpp "$REF"/code/eshared/synth/*.hpp "$TREE/synth/"
mkdir -p "$TREE/synth/directx"
cp "$REF"/code/eshared/synth/directx/tf_soundoutdx8.hpp "$TREE/synth/directx/"
cp -R "$here/shim/system" "$here/shim/math" "$here/shim/eshared.hpp" "$TREE/"
SRCS=(addsynth adsr filter functions ieffect instrument lfo modmatrix noise oscillator player song)
for cfg in player editor; do
  if [ "$cfg" = player ]; then DEF=(-DePLAYER); else DEF=(-DeEDITOR); fi
  mkdir -p "$OUT/obj-$cfg"
  objs=()
  for s in "${SRCS[@]}"; do
    o="$OUT/obj-$cfg/tf_$s.o"
    clang++ -std=c++17 -O2 -msse2 -w "${DEF[@]}" -I"$TREE" -c "$TREE/synth/tf_$s.cpp" -o "$o"
    objs+=("$o")
  done
  clang++ -std=c++17 -O2 -msse2 -w "${DEF[@]}" -I"$TREE" -c "$here/render-ref.cpp" -o "$OUT/obj-$cfg/render-ref.o"
  clang++ -o "$OUT/render-ref-$cfg" "${objs[@]}" "$OUT/obj-$cfg/render-ref.o"
  echo "$OUT/render-ref-$cfg"
done
# the point oscillator's cycle alone (player objects; the spline has no editor-only part)
clang++ -std=c++17 -O2 -msse2 -w -DePLAYER -I"$TREE" -c "$here/cycle-ref.cpp" -o "$OUT/obj-player/cycle-ref.o"
clang++ -o "$OUT/cycle-ref" $(ls "$OUT"/obj-player/tf_*.o) "$OUT/obj-player/cycle-ref.o"
echo "$OUT/cycle-ref"
clang++ -std=c++17 -O2 -msse2 -w -DePLAYER -I"$TREE" -c "$here/pad-ref.cpp" -o "$OUT/obj-player/pad-ref.o"
clang++ -o "$OUT/pad-ref" $(ls "$OUT"/obj-player/tf_*.o) "$OUT/obj-player/pad-ref.o"
echo "$OUT/pad-ref"
