#!/usr/bin/env bash
# Fetches the audio of public YouTube captures of the intros played by Mentor's 4k synth (Elevated, Atrium) into a
# local directory OUTSIDE the repo (change add-elevated-import; the music belongs to its authors: never committed).
#   ELEVATED_CAPTURES=<dir> (default ~/Downloads/64k-tools/elevated-captures)
# Needs uvx (runs yt-dlp without installing it) and ffmpeg; exits 0 with a message when either is missing.
# Prints each file's SHA-256, its length and where its music starts (first sound above −60 dBFS).
set -euo pipefail
DIR="${ELEVATED_CAPTURES:-$HOME/Downloads/64k-tools/elevated-captures}"
# slug, video id (Annikras HQ captures, checked 2026-11-10)
CAPTURES=("elevated jB0vBmiTr6o" "atrium jlRoOqRs0bs")
for tool in uvx ffmpeg; do
  if ! command -v "$tool" >/dev/null; then
    echo "elevated captures: skipped ($tool not found)"
    exit 0
  fi
done
mkdir -p "$DIR"
for entry in "${CAPTURES[@]}"; do
  set -- $entry
  slug=$1 id=$2 wav="$DIR/$1.wav"
  if [ ! -f "$wav" ]; then
    uvx yt-dlp -q --no-warnings -f bestaudio -x --audio-format wav --postprocessor-args "-ar 44100 -ac 2" \
      -o "$DIR/$slug.%(ext)s" "https://www.youtube.com/watch?v=$id"
  fi
  start=$(ffmpeg -nostats -i "$wav" -af "silencedetect=n=-60dB:d=0.3" -f null - 2>&1 |
    awk '/silence_start: 0($|[^.0-9])/ {s=1} s && /silence_end/ {print $5; exit}')
  len=$(ffprobe -v error -show_entries format=duration -of default=nw=1:nk=1 "$wav")
  sum=$(shasum -a 256 "$wav" | cut -c1-16)
  echo "$slug  $id  sha256 $sum…  ${len} s  music starts at ${start:-0} s  $wav"
done
