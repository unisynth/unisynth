#!/usr/bin/env bash
# Marginal code cost of each unisynth module: the all-modules i686 probe (compo configuration: core rebuilt with
# optimize_for_size, opt-level z, LTO) against the same build with one leaf feature removed. The song is not rendered
# correctly without the feature, only the code that is compiled in matters. Writes out/marginal.csv (feature,raw,xz).
#   ./marginal.sh [feature …]        default: every leaf feature
set -euo pipefail
cd "$(dirname "$0")"
export PATH="$HOME/.cargo/bin:$PATH"
OUT=out; mkdir -p "$OUT"
SONG="$PWD/$OUT/songs/showcase.song"; [ -f "$SONG" ] || { echo "run ./measure.sh first (it writes $SONG)" >&2; exit 1; }
F="-Clinker-flavor=lld-link -Ctarget-cpu=pentium4 -Crelocation-model=static -Zlocation-detail=none -Zunstable-options -Cpanic=immediate-abort --cfg builtin_mem"
for a in /ENTRY:start /SUBSYSTEM:WINDOWS /NODEFAULTLIB /DEBUG:NONE /OPT:REF /OPT:ICF /SAFESEH:NO \
         /BASE:0x400000 /DYNAMICBASE:NO /FIXED; do F="$F -Clink-arg=$a"; done
LEAVES=(us-osc-pm us-osc-blit us-osc-wt us-osc-sample us-sample-gsm us-sample-dls us-sample-stereo us-noise us-flt-svf us-flt-ladder us-flt-rbj us-flt-notch
  us-env-seconds us-env-rate us-voice-lfo us-voice-glide us-voice-mono us-voice-unison us-voice-oneshot us-matrix us-automation us-fx-distortion-shaper us-fx-distortion-power us-fx-crusher us-fx-eq-parametric us-fx-eq-3band us-fx-delay-tempo
  us-fx-delay-ms us-fx-chorus us-fx-flanger-phaser us-fx-flanger-ring us-fx-reverb-freeverb us-fx-reverb-compact us-fx-reverb-rooms
  us-fx-compressor us-fx-formant)
[ $# -gt 0 ] && LEAVES=("$@")
build() { # features(csv, without unisynth/ prefix) → raw xz
  US_SONG="$SONG" RUSTFLAGS="$F" CARGO_PROFILE_RELEASE_OPT_LEVEL=z cargo build -q --release \
    --features "$(echo "$1" | tr ',' '\n' | sed 's|^|unisynth/|' | paste -sd, -)" --target i686-pc-windows-msvc \
    -Zbuild-std=core,compiler_builtins -Zbuild-std-features=compiler-builtins-mem,optimize_for_size >&2
  local exe=target/i686-pc-windows-msvc/release/us-size-probe.exe
  echo "$(stat -f%z $exe),$(xz -9e -c $exe | wc -c | tr -d ' ')"
}
ALL=$(IFS=,; echo "${LEAVES_ALL:-${LEAVES[*]}}")
[ -n "${LEAVES_ALL:-}" ] || ALL=$(IFS=,; echo "us-osc-pm,us-osc-blit,us-osc-wt,us-osc-sample,us-sample-gsm,us-sample-dls,us-noise,us-flt-svf,us-flt-ladder,us-flt-rbj,us-flt-notch,us-env-seconds,us-env-rate,us-voice-lfo,us-voice-glide,us-voice-mono,us-voice-unison,us-voice-oneshot,us-matrix,us-automation,us-fx-distortion-shaper,us-fx-distortion-power,us-fx-crusher,us-fx-eq-parametric,us-fx-eq-3band,us-fx-delay-tempo,us-fx-delay-ms,us-fx-chorus,us-fx-flanger-phaser,us-fx-flanger-ring,us-fx-reverb-freeverb,us-fx-reverb-compact,us-fx-reverb-rooms,us-fx-compressor,us-fx-formant")
echo "feature,raw,xz" > "$OUT/marginal.csv"
echo "all,$(build "$ALL")" >> "$OUT/marginal.csv"; tail -1 "$OUT/marginal.csv"
for f in "${LEAVES[@]}"; do
  rest=$(echo "$ALL" | tr ',' '\n' | grep -vx "$f" | paste -sd, -)
  echo "$f,$(build "$rest")" >> "$OUT/marginal.csv"; tail -1 "$OUT/marginal.csv"
done
