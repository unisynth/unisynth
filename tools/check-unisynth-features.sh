#!/usr/bin/env bash
# Builds `unisynth` with every single-module feature set (no default features, one module and what it implies),
# plus none and all, for i686 core-only (the player target, no std) and wasm32, and runs the host tests of each set.
# Fails on any error or warning. Usage: tools/check-unisynth-features.sh [--no-test]
set -uo pipefail
cd "$(dirname "$0")/.."
export PATH="$HOME/.cargo/bin:$PATH"
MODULES="us-osc-pm us-osc-blit us-osc-wt us-osc-sample us-sample-gsm us-sample-dls us-noise
         us-flt-svf us-flt-ladder us-flt-rbj us-flt-notch us-env-timed us-env-rate"
FX="us-fx-distortion-shaper us-fx-distortion-power us-fx-crusher us-fx-eq-parametric us-fx-eq-3band us-fx-delay-tempo
    us-fx-delay-ms us-fx-chorus us-fx-flanger-phaser us-fx-flanger-ring us-fx-reverb-freeverb us-fx-reverb-compact
    us-fx-reverb-rooms us-fx-compressor us-fx-formant"
SETS=("" $MODULES $FX "us-osc" "us-flt" "us-env" "us-fx" "us-all")
fail=0
run() { # name, args...
  local out; out=$("$@" 2>&1); local rc=$?
  if [ $rc -ne 0 ] || grep -q '^warning' <<<"$out"; then
    echo "FAIL  $name"; grep -E '^(error|warning)' -A6 <<<"$out" | head -20; fail=1
  fi
}
for set in "${SETS[@]}"; do
  name="[${set:-none}]"
  f=(--no-default-features ${set:+--features "$set"})
  run cargo check -q -p unisynth "${f[@]}" --target i686-pc-windows-msvc -Zbuild-std=core
  run cargo check -q -p unisynth "${f[@]}" --target wasm32-unknown-unknown
  [ "${1:-}" = "--no-test" ] || run cargo test -q -p unisynth "${f[@]}"
  echo "ok    $name"
done
exit $fail
