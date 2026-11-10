#!/usr/bin/env bash
# Builds `unisynth` with every single-module feature set (no default features, one module and what it implies),
# plus none and all, for i686 core-only (the player target, no std) and wasm32, and runs the host tests of each set.
# Fails on any error or warning. Usage: tools/check-unisynth-features.sh [--no-test]
set -uo pipefail
cd "$(dirname "$0")/.."
export PATH="$HOME/.cargo/bin:$PATH"
MODULES="us-osc-pm us-osc-blit us-osc-wt us-osc-sample us-sample-gsm us-sample-dls us-sample-stereo us-noise
         us-flt-svf us-flt-ladder us-flt-rbj us-flt-notch us-env-seconds us-env-rate
         us-voice-lfo us-voice-glide us-voice-mono us-voice-unison us-voice-oneshot us-matrix us-matrix-add us-speech us-automation
         us-osc-naive us-flt-feedback us-flt-parallel us-drive us-env-frame us-lfo-frame us-voice-out us-strip us-chunk-wide
         us-osc-combine us-quirk-exact-math us-quirk-integer-fm us-quirk-fixed-seed us-quirk-lcg-noise us-quirk-coarse-env
         us-osc-points us-osc-pad us-osc-pair us-voice-drop us-env-linear us-osc-shaper-tone us-osc-sweep-drum"
FX="us-fx-distortion-shaper us-fx-distortion-curve us-fx-distortion-power us-fx-crusher us-fx-eq-parametric us-fx-eq-3band us-fx-delay-tempo
    us-fx-delay-ms us-fx-chorus us-fx-chorus-tf3 us-fx-flanger-phaser us-fx-flanger-ring us-fx-reverb-freeverb us-fx-reverb-compact
    us-fx-reverb-rooms us-fx-compressor us-fx-formant us-fx-eq-cut us-fx-delay-modulated us-fx-reverb-comb us-fx-compressor-lookahead
    us-fx-distortion-atan us-fx-distortion-sine us-fx-distortion-knee us-fx-eq-svf-lfo us-fx-delay-pingpong us-fx-delay-allpass"
SETS=("" $MODULES $FX "us-osc" "us-flt" "us-env" "us-voice" "us-fx" "us-all"
      "us-osc,us-noise,us-flt,us-env,us-fx" "us-osc,us-noise,us-flt,us-env,us-voice,us-fx" "us-osc,us-noise,us-flt,us-env,us-voice,us-matrix,us-fx")  # the last three: everything but the voice paths / the matrix / automation (gated tests, goldens)
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
# `us-trusted` (compo builds of generated songs) leaves validation out, so tests that expect BadChunk cannot pass: build only
for set in "us-all,us-trusted" "us-noise,us-flt-svf,us-env-seconds,us-trusted"; do
  name="[$set] (build only)"
  run cargo check -q -p unisynth --no-default-features --features "$set" --target i686-pc-windows-msvc -Zbuild-std=core
  run cargo check -q -p unisynth --no-default-features --features "$set" --target wasm32-unknown-unknown
  echo "ok    $name"
done
exit $fail
