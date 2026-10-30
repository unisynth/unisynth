#!/usr/bin/env bash
# Per-song packed size of the compo probe for one tree (change unify-v2-algorithms, gate D7.6): builds the i686 probe
# baked with each song's own feature list (measure.sh's compo flags), prints raw / xz -9e / squishy 0.1.3 bytes.
# Songs: the seven reference songs from out/songs plus V2 songs converted by this tree's synth-convert
# (fr08 brullwurfel candytron kkrieger debris platinum; skipped when $V2_SONGS is absent).
#   research/us-size-probe/songs-size.sh [tree]     (default: this checkout)
set -u
T=$(cd "${1:-$(dirname "$0")/../..}" && pwd)
export PATH="$HOME/.cargo/bin:$PATH" WINEDEBUG=-all WINEPREFIX="${WINEPREFIX:-$HOME/.wine-64k}"
W="${WINE64:-/Applications/Wine Stable.app/Contents/Resources/wine/bin/wine64}"
SQ=${SQUISHY:-$HOME/Downloads/64k-tools/squishy-0.1.3/squishy-x64.exe}
S=${V2_SONGS:-$HOME/Downloads/64k-tools/synth-lab/v2songs}
D=$(mktemp -d)
(cd "$T" && CARGO_TARGET_DIR="$T/target" cargo build -q --release -p synth-convert --bin synth-convert)
C="$T/target/release/synth-convert"
for n in global-groove elysian engage eladamri no-remorse tf4testsong turtles; do cp "$T/research/us-size-probe/out/songs/$n.song" "$D/"; done
v2() { [ -f "$2" ] && "$C" v2m "$2" "$D/v2-$1.song" 4 2>/dev/null; }
v2 fr08 "$S/acceptance/fr08.v2m"; v2 brullwurfel "$S/acceptance/brullwurfel.v2m"; v2 candytron "$S/acceptance/candytron.v2m"
v2 kkrieger "$S/acceptance/kkrieger.v2m"; v2 debris "$S/fr_public/werkkzeug3__data__debris__debris_ost.v2m"
v2 platinum "$S/modland/KB--fr-052_platinum.v2m"
F="-Clinker-flavor=lld-link -Ctarget-cpu=pentium4 -Crelocation-model=static -Zlocation-detail=none"
for a in /ENTRY:start /SUBSYSTEM:WINDOWS /NODEFAULTLIB /DEBUG:NONE /OPT:REF /OPT:ICF /SAFESEH:NO /BASE:0x400000 /DYNAMICBASE:NO /FIXED; do F="$F -Clink-arg=$a"; done
cd "$T/research/us-size-probe"
for s in "$D"/*.song; do
  feats=$("$C" features "$s" 2>/dev/null | head -1 | tr ',' '\n' | sed 's|^|unisynth/|' | paste -sd, -)
  US_SONG="$s" RUSTFLAGS="$F" cargo build -q --release --features "baked${feats:+,$feats}" --target i686-pc-windows-msvc || { echo "$(basename "$s"): build failed"; continue; }
  e=target/i686-pc-windows-msvc/release/us-size-probe.exe
  rm -f "$D/sq.exe"; [ -x "$W" ] && [ -f "$SQ" ] && "$W" "$SQ" -i "$e" -o "$D/sq.exe" >/dev/null 2>&1
  echo "$(basename "$s" .song) raw=$(stat -f%z "$e") xz=$(xz -9e -c "$e" | wc -c | tr -d ' ') squishy=$(stat -f%z "$D/sq.exe" 2>/dev/null || echo -)"
done
rm -rf "$D"
