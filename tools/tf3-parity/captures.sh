#!/usr/bin/env bash
# Fetches the audio of public YouTube captures of Brain Control's Tunefish 3 intros into a local directory OUTSIDE the
# repo (design D2a of add-tf3-import; the music belongs to its authors: never committed).
#   TF3_CAPTURES=<dir> (default ~/Downloads/64k-tools/tf3-captures)
# Needs uvx (runs yt-dlp without installing it) and ffmpeg; exits 0 with a message when either is missing.
# Prints each file's SHA-256 and where its music starts (first sound above −60 dBFS).
set -euo pipefail
DIR="${TF3_CAPTURES:-$HOME/Downloads/64k-tools/tf3-captures}"
# slug, video id (Annikras HQ captures, checked 2026-11-09)
CAPTURES=("transplant 31cDnVWhS7w" "pandora RlyIJD9AEWM" "train-control Gm0NWMdcbmU")
for tool in uvx ffmpeg; do
  if ! command -v "$tool" >/dev/null; then
    echo "tf3 captures: skipped ($tool not found)"
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
  sum=$(shasum -a 256 "$wav" | cut -c1-16)
  echo "$slug  $id  sha256 $sum…  music starts at ${start:-0} s  $wav"
done
