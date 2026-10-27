#!/usr/bin/env bash
# Code cost of the optional voice paths (task 4.2 of shrink-unified-synth): the i686 probe in the compo configuration
# (as marginal.sh) built normally and with one path stubbed out through `--cfg us_ablate="<switch>"`. The stubs are
# temporary edits in unisynth/synth-core and are not committed; a switch the code does not know builds the baseline.
# Writes out/ablate.csv (song,switch,raw,xz).
#   ./ablate.sh [switch …]     default: unison mono glide lfo ctl oneshot spread fine steal
#   SONGS="all-modules:out/songs/showcase.song no-remorse:out/songs/no-remorse.song" (defaults: the songs in out/songs)
set -euo pipefail
cd "$(dirname "$0")"
export PATH="$HOME/.cargo/bin:$PATH"
OUT=out; mkdir -p "$OUT"
CONVERT=../../target/release/synth-convert
F="-Clinker-flavor=lld-link -Ctarget-cpu=pentium4 -Crelocation-model=static -Zlocation-detail=none -Zunstable-options -Cpanic=immediate-abort --cfg builtin_mem"
for a in /ENTRY:start /SUBSYSTEM:WINDOWS /NODEFAULTLIB /DEBUG:NONE /OPT:REF /OPT:ICF /SAFESEH:NO \
         /BASE:0x400000 /DYNAMICBASE:NO /FIXED; do F="$F -Clink-arg=$a"; done
SW=(unison mono glide lfo ctl oneshot spread fine steal)
[ $# -gt 0 ] && SW=("$@")
if [ -z "${SONGS:-}" ]; then
  SONGS=""
  for s in "$OUT"/songs/*.song; do n=$(basename "$s" .song); [ "$n" = showcase ] && n=all-modules; SONGS="$SONGS $n:$s"; done
fi
build() { # song features switch → raw,xz
  US_SONG="$PWD/$1" RUSTFLAGS="$F $(for x in ${3//+/ }; do printf -- "--cfg us_ablate=\"%s\" " "$x"; done)" CARGO_PROFILE_RELEASE_OPT_LEVEL=z cargo build -q --release \
    --features "$(echo "$2" | tr ',' '\n' | sed 's|^|unisynth/|' | paste -sd, -)" --target i686-pc-windows-msvc \
    -Zbuild-std=core,compiler_builtins -Zbuild-std-features=compiler-builtins-mem,optimize_for_size 2>/dev/null >&2
  local exe=target/i686-pc-windows-msvc/release/us-size-probe.exe
  echo "$(stat -f%z $exe),$(xz -9e -c $exe | wc -c | tr -d ' ')"
}
echo "song,switch,raw,xz" > "$OUT/ablate.csv"
for e in $SONGS; do
  name=${e%%:*} song=${e#*:}
  feats=us-all; [ "$name" = all-modules ] || feats=$("$CONVERT" features "$song")
  echo "$name,base,$(build "$song" "$feats" none)" >> "$OUT/ablate.csv"; tail -1 "$OUT/ablate.csv"
  for s in "${SW[@]}"; do echo "$name,$s,$(build "$song" "$feats" "$s")" >> "$OUT/ablate.csv"; tail -1 "$OUT/ablate.csv"; done
done
