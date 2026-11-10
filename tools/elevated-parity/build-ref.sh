#!/usr/bin/env bash
# Reference renders of songs by Mentor's 4k synth (change add-elevated-import, design D9). The original synth.asm and
# the song's music.asm/music.h are copied OUTSIDE the repo into $ELEVATED_REF/<song>/, assembled with nasm -f win32,
# linked with our MIT harness render-ref.c (i686 MinGW) and run under Wine in a Linux container. Nothing original is
# committed; the sources stay where the user keeps them.
#   build-ref.sh [song …]            songs: atrium, elevated (default both)
#   ELEVATED_SRC  Atrium's synth.asm + music.asm + music.h (default ~/Downloads/zenekk/TBC - Atrium)
#   ELEVATED_REF  work dir (default /tmp/elevated-ref); Elevated's public source is cloned into it
#   SOLO=1        also render every synth machine alone: <song>-solo<k>.wav
#   DOCKER        docker binary (default: docker on PATH, else Docker.app's)
# Prints each WAV; exits 0 with a message when a song's sources or docker are missing.
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
SRC="${ELEVATED_SRC:-$HOME/Downloads/zenekk/TBC - Atrium}"
REF="${ELEVATED_REF:-/tmp/elevated-ref}"
# Elevated (pouet 52938): the public source release, mirrored by in4k; pinned
ELEV_REPO=https://github.com/in4k/rgba_tbc_elevated_source
ELEV_REV=1832f2b58f0896901483ab52e7d18a4b489bd7f4
DOCKER="${DOCKER:-$(command -v docker || echo /Applications/Docker.app/Contents/Resources/bin/docker)}"
IMAGE=elevated-ref
if [ ! -x "$DOCKER" ] || ! "$DOCKER" info >/dev/null 2>&1; then
  echo "elevated reference: skipped (docker not available)"
  exit 0
fi
"$DOCKER" image inspect "$IMAGE" >/dev/null 2>&1 || "$DOCKER" build -q -t "$IMAGE" "$HERE" >/dev/null
mkdir -p "$REF"

# song → directory holding synth.asm (+ synth_core.nh), music.asm, music.h; and its machine table size
locate() {
  case "$1" in
    atrium) DIR="$SRC"; TABLE=8 ;;
    elevated)
      if [ ! -d "$REF/elevated-src/.git" ]; then
        git clone -q "$ELEV_REPO" "$REF/elevated-src" && git -C "$REF/elevated-src" checkout -q "$ELEV_REV"
      fi
      DIR="$REF/elevated-src/src"; TABLE=7 ;;
    *) echo "unknown song $1" >&2; exit 2 ;;
  esac
}

for song in "${@:-atrium elevated}"; do
  for s in $song; do
    locate "$s"
    if [ ! -f "$DIR/synth.asm" ] || [ ! -f "$DIR/music.asm" ] || [ ! -f "$DIR/music.h" ]; then
      echo "$s: skipped (no synth.asm/music.asm/music.h in $DIR)"
      continue
    fi
    W="$REF/$s"
    rm -rf "$W" && mkdir -p "$W/src"
    cp "$DIR"/music.asm "$DIR"/music.h "$W/src/"
    [ -f "$DIR/synth_core.nh" ] && cp "$DIR/synth_core.nh" "$W/src/"
    # the includes name src\x with a backslash; nasm on Linux wants a slash
    tr -d '\r' < "$DIR/synth.asm" | sed 's#src\\#src/#' > "$W/synth.asm"
    [ -f "$W/src/synth_core.nh" ] && sed -i.bak 's#src\\#src/#' "$W/src/synth_core.nh"
    # music.h's constants as C #defines for the harness
    tr -d '\r' < "$DIR/music.h" | sed -n 's/^static const int \([A-Z_]*\) equ \(.*\);.*/#define \1 (\2)/p' > "$W/music_c.h"
    cp "$HERE/render-ref.c" "$W/"
    solos=""
    [ "${SOLO:-0}" = 1 ] && solos=1
    "$DOCKER" run --rm -v "$W":/w -w /w -e TABLE="$TABLE" -e SOLOS="$solos" -e SONG="$s" "$IMAGE" sh -c '
      set -e
      nasm -f win32 synth.asm -o synth.obj
      i686-w64-mingw32-gcc -O2 -o render-ref.exe render-ref.c synth.obj
      wine render-ref.exe "$SONG.wav" -t "$TABLE"
      if [ -n "$SOLOS" ]; then
        k=0
        while wine render-ref.exe "$SONG-solo$k.wav" -t "$TABLE" --solo "$k" 2>/dev/null; do k=$((k + 1)); done
        rm -f "$SONG-solo$k.wav"
      fi'
    ls "$W"/*.wav
  done
done
