#!/usr/bin/env bash
# The 6.4 freeze gate (change unify-v2-algorithms): the compo probe of each reference song (and the V2 probe songs,
# recorded) built twice from the same player code: with the song's own bytes, and with the v3 layout's bytes
# (`layout.py blob`: instrument chunks migrated, lanes renumbered, effect chunks marked) plus the 34 defaults the
# layout adds (`--cfg us_v3_pad`). Prints raw / xz -9e / squishy per song and variant.
#   research/unify-v3-layout/probe.sh            (ONLY=<song regex> measures a subset)
set -u
H=$(cd "$(dirname "$0")" && pwd); T=$(cd "$H/../.." && pwd)
export PATH="$HOME/.cargo/bin:$PATH" WINEDEBUG=-all WINEPREFIX="${WINEPREFIX:-$HOME/.wine-64k}"
W="${WINE64:-/Applications/Wine Stable.app/Contents/Resources/wine/bin/wine64}"
SQ=${SQUISHY:-$HOME/Downloads/64k-tools/squishy-0.1.3/squishy-x64.exe}
S=${V2_SONGS:-$HOME/Downloads/64k-tools/synth-lab/v2songs}
D=$(mktemp -d)
(cd "$T" && cargo build -q --release -p synth-convert --bin synth-convert)
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
  n=$(basename "$s" .song)
  [ -n "${ONLY:-}" ] && ! [[ "$n" =~ ^($ONLY)$ ]] && continue
  feats=$("$C" features "$s" 2>/dev/null | head -1 | tr ',' '\n' | sed 's|^|unisynth/|' | paste -sd, -)
  "$C" rust "$s" "$D/$n.old.rs" >/dev/null || { echo "$n: rust failed"; continue; }
  python3 "$H/layout.py" blob "$s" "$D/$n.v3.song" && python3 "$H/layout.py" rust "$D/$n.old.rs" "$D/$n.v3.song" "$D/$n.v3.rs" || { echo "$n: migrate failed"; continue; }
  line="$n"
  for v in old v3; do
    flags="$F"; [ $v = v3 ] && flags="$F --cfg us_v3_pad"
    US_SONG="$s" US_SONG_RS="$D/$n.$v.rs" RUSTFLAGS="$flags" CARGO_TARGET_DIR="$T/research/us-size-probe/target-$v" \
      cargo build -q --release --features "baked${feats:+,$feats}" --target i686-pc-windows-msvc || { line="$line $v=build-failed"; continue; }
    e="$T/research/us-size-probe/target-$v/i686-pc-windows-msvc/release/us-size-probe.exe"
    rm -f "$D/sq.exe"; [ -x "$W" ] && [ -f "$SQ" ] && "$W" "$SQ" -i "$e" -o "$D/sq.exe" >/dev/null 2>&1
    line="$line $v:raw=$(stat -f%z "$e"),xz=$(xz -9e -c "$e" | wc -c | tr -d ' '),squishy=$(stat -f%z "$D/sq.exe" 2>/dev/null || echo -)"
  done
  echo "$line"
done
rm -rf "$D"
