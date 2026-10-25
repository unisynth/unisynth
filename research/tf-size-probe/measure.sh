#!/usr/bin/env bash
# tfsynth size probe: builds the i686 no_std probe with all effect kinds (`all`) and with none (default),
# prints raw size, xz -9e size (packer-independent proxy) and squishy 0.1.3 packed size when Wine + the tool exist.
#   ./measure.sh            (TOOLS=~/Downloads/64k-tools, WINE64=… as in ../bootstrap-probe/pack.sh)
set -euo pipefail
cd "$(dirname "$0")"
export PATH="$HOME/.cargo/bin:$PATH"
OUT=out; mkdir -p "$OUT/packed"
TOOLS="${TOOLS:-$HOME/Downloads/64k-tools}"
WINE64="${WINE64:-/Applications/Wine Stable.app/Contents/Resources/wine/bin/wine64}"
export WINEDEBUG=-all WINEPREFIX="${WINEPREFIX:-$HOME/.wine-64k}"
F="-Clinker-flavor=lld-link -Ctarget-cpu=pentium4 -Crelocation-model=static -Zlocation-detail=none"
for a in /ENTRY:start /SUBSYSTEM:WINDOWS /NODEFAULTLIB /DEBUG:NONE /OPT:REF /OPT:ICF /SAFESEH:NO \
         /BASE:0x400000 /DYNAMICBASE:NO /FIXED; do F="$F -Clink-arg=$a"; done
sz() { [ -f "$1" ] && stat -f%z "$1" || echo -; }
printf '%-12s %9s %9s %9s\n' build raw xz-9e squishy
for feat in all none; do
  farg=(); [ "$feat" = all ] && farg=(--features all)
  RUSTFLAGS="$F" cargo build -q --release ${farg[@]+"${farg[@]}"} --target i686-pc-windows-msvc
  exe="$OUT/tf-$feat.exe"; cp target/i686-pc-windows-msvc/release/tf-size-probe.exe "$exe"
  xz -9e -k -c "$exe" > "$OUT/tf-$feat.exe.xz"
  sq="$OUT/packed/tf-$feat.squishy013.exe"; rm -f "$sq"
  if [ -x "$WINE64" ] && [ -f "$TOOLS/squishy-0.1.3/squishy-x64.exe" ]; then
    "$WINE64" "$TOOLS/squishy-0.1.3/squishy-x64.exe" -i "$exe" -o "$sq" >/dev/null 2>&1 || true
  fi
  printf '%-12s %9s %9s %9s\n' "$feat" "$(sz "$exe")" "$(sz "$OUT/tf-$feat.exe.xz")" "$(sz "$sq")"
done
