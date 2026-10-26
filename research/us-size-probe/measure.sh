#!/usr/bin/env bash
# Unified-synth size probe: builds the i686 no_std probe for the all-modules build (the converted WaveSabre `showcase`
# song, every feature) and for each reference song with exactly the features `synth-convert features` lists for it, prints
# raw size, xz -9e size (packer-independent proxy) and squishy 0.1.3 size when Wine + the tool exist, and writes the
# table to ../../tools/unisynth-parity/size.md for the parity report.
#   WS_SONG_DIR=… TF_SONGS=… ./measure.sh      (songs that are not there are skipped; TOOLS, WINE64 as in ../ws-size-probe)
set -euo pipefail
cd "$(dirname "$0")"
ROOT=$(cd ../.. && pwd)
export PATH="$HOME/.cargo/bin:$PATH"
OUT=out; mkdir -p "$OUT/packed" "$OUT/songs"
TOOLS="${TOOLS:-$HOME/Downloads/64k-tools}"
WINE64="${WINE64:-/Applications/Wine Stable.app/Contents/Resources/wine/bin/wine64}"
export WINEDEBUG=-all WINEPREFIX="${WINEPREFIX:-$HOME/.wine-64k}"
F="-Clinker-flavor=lld-link -Ctarget-cpu=pentium4 -Crelocation-model=static -Zlocation-detail=none"
for a in /ENTRY:start /SUBSYSTEM:WINDOWS /NODEFAULTLIB /DEBUG:NONE /OPT:REF /OPT:ICF /SAFESEH:NO \
         /BASE:0x400000 /DYNAMICBASE:NO /FIXED; do F="$F -Clink-arg=$a"; done
sz() { [ -f "$1" ] && stat -f%z "$1" || echo -; }
CONVERT="$ROOT/target/release/synth-convert"
(cd "$ROOT" && cargo build -q --release -p synth-convert --bin synth-convert)

# the converted showcase song (every WaveSabre device) for the all-modules build
(cd "$ROOT" && FEATURES_DUMP="$PWD/research/us-size-probe/$OUT/songs" cargo test -q -p synth-convert --test features dump_songs_for_the_compo_build_check >/dev/null)
rows=()
build() { # name song features
  local name=$1 song=$2 feats=$3 fl=""
  [ -n "$feats" ] && fl=$(echo "$feats" | tr ',' '\n' | sed 's|^|unisynth/|' | paste -sd, -)
  US_SONG="$song" RUSTFLAGS="$F" cargo build -q --release ${fl:+--features "$fl"} --target i686-pc-windows-msvc
  local exe="$OUT/us-$name.exe"; cp target/i686-pc-windows-msvc/release/us-size-probe.exe "$exe"
  xz -9e -k -c "$exe" > "$exe.xz"
  local sq="$OUT/packed/us-$name.squishy013.exe"; rm -f "$sq"
  if [ -x "$WINE64" ] && [ -f "$TOOLS/squishy-0.1.3/squishy-x64.exe" ]; then
    "$WINE64" "$TOOLS/squishy-0.1.3/squishy-x64.exe" -i "$exe" -o "$sq" >/dev/null 2>&1 || true
  fi
  printf '| %s | %s | %s | %s | %s | %s |\n' "$name" "$(stat -f%z "$song")" "$(sz "$exe")" "$(sz "$exe.xz")" "$(sz "$sq")" "${feats:-all}"
}
ALL=us-all
rows+=("$(build all-modules "$OUT/songs/showcase.song" "$ALL")")
if [ -n "${WS_SONG_DIR:-}" ]; then
  for n in groove:global-groove elysian:elysian engage:engage eladamri:eladamri; do
    [ -f "$WS_SONG_DIR/${n%%:*}.bin" ] || continue
    "$CONVERT" ws "$WS_SONG_DIR/${n%%:*}.bin" "$OUT/songs/${n##*:}.song" 2>/dev/null
    rows+=("$(build "${n##*:}" "$OUT/songs/${n##*:}.song" "$("$CONVERT" features "$OUT/songs/${n##*:}.song")")")
  done
fi
if [ -n "${TF_SONGS:-}" ]; then
  for n in no_remorse:no-remorse tf4testsong:tf4testsong; do
    [ -f "$TF_SONGS/${n%%:*}.tfm" ] || continue
    "$CONVERT" tfm "$TF_SONGS/${n%%:*}.tfm" "$OUT/songs/${n##*:}.song" 2>/dev/null
    rows+=("$(build "${n##*:}" "$OUT/songs/${n##*:}.song" "$("$CONVERT" features "$OUT/songs/${n##*:}.song")")")
  done
fi
{
  echo "i686 \`no_std\` probe, opt-level z, LTO, static base (\`research/us-size-probe\`, \`measure.sh\`). Raw and packed bytes include the song's own"
  echo "bytes (the \"song\" column) and the pure-Rust \`libm\`; the build lists exactly the features \`synth-convert features\` names for the song."
  echo
  echo "| build | song B | raw B | xz -9e B | squishy 0.1.3 B | features |"
  echo "|---|---:|---:|---:|---:|---|"
  printf '%s\n' "${rows[@]}"
  echo
  echo "Legacy figures (\`research/softsynth-research.md\`): WaveSabre port, all 13 devices + 1 KB test sample: 71,168 raw / 34,472 xz / 27,648 squishy;"
  echo "Slaughter only: 28,672 / 15,968 / 14,336. Tunefish-compatible port: 41,472 raw with every effect, 30,208 without."
} > "$ROOT/tools/unisynth-parity/size.md"
cat "$ROOT/tools/unisynth-parity/size.md"
