#!/usr/bin/env bash
# Builds the upstream Tunefish 4 reference renderer for tools/tf-parity, OUTSIDE the repo (GPL-3: never committed).
#   TF_REF=<dir> (default /tmp/tf-ref): upstream checkout; cloned at the pinned commit if absent.
# Output: $TF_REF/build/render-ref (our MIT harness render-ref.cpp linked with the upstream player + synth).
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
REF="${TF_REF:-/tmp/tf-ref}"
COMMIT=7e48ce8
if [ ! -d "$REF/src/tunefish4" ]; then
  git clone -q https://github.com/paynebc/tunefish "$REF"
  git -C "$REF" checkout -q "$COMMIT"
fi
S="$REF/src"
OUT="$REF/build"; mkdir -p "$OUT/obj"
INC=(-I"$S/tunefish4player" -I"$S/tunefish4/Source/synth" -I"$S/tunefish4/Source/runtime" -I"$S/tunefish4/Source")
SRCS=("$S"/tunefish4/Source/runtime/{array,random,runtime,simd}.cpp "$S"/tunefish4/Source/synth/{tf4,tf4fx}.cpp
      "$S"/tunefish4player/{tf4player,datastream}.cpp "$here/render-ref.cpp")
objs=()
for src in "${SRCS[@]}"; do
  o="$OUT/obj/$(basename "${src%.cpp}").o"
  if [ ! -f "$o" ] || [ "$src" -nt "$o" ]; then
    clang++ -std=c++17 -O2 -w "${INC[@]}" -c "$src" -o "$o"
  fi
  objs+=("$o")
done
clang++ -o "$OUT/render-ref" "${objs[@]}"
echo "$OUT/render-ref"
