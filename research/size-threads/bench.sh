#!/usr/bin/env bash
# add-synth-threads benchmark: wall time of `song render` (release) per song and worker count, and a byte comparison
# of every WAV with the single-thread one. Prints a markdown table.
#   ./bench.sh [songs…]   (default: the dense project/music songs)
set -euo pipefail
cd "$(dirname "$0")"
ROOT=$(cd ../.. && pwd)
export PATH="$HOME/.cargo/bin:$PATH"
(cd "$ROOT" && cargo build -q --release -p song)
SONG="$ROOT/target/release/song"
mkdir -p out/bench
songs=("${@:-engage elysian turtles no-remorse eladamri}")
echo "| Song | flag none | 1 | 2 | 4 | 8 | 16 | best speed-up | WAVs equal |"
echo "|---|---|---|---|---|---|---|---|---|"
for s in ${songs[@]}; do
  "$SONG" export "$ROOT/project/music/$s.song.json" -o "out/bench/$s.bin"
  row="| \`$s\`"; base=""; best=""; eq=yes
  for n in none 1 2 4 8 16; do
    args=(); [ "$n" != none ] && args=(--threads "$n")
    t0=$(perl -MTime::HiRes=time -e 'printf "%.3f", time')
    "$SONG" render "out/bench/$s.bin" -o "out/bench/$s-$n.wav" ${args[@]+"${args[@]}"}
    t1=$(perl -MTime::HiRes=time -e 'printf "%.3f", time')
    t=$(echo "$t1 - $t0" | bc)
    row="$row | $t"
    [ -z "$base" ] && base=$t
    if [ -z "$best" ] || [ "$(echo "$t < $best" | bc)" = 1 ]; then best=$t; fi
    cmp -s "out/bench/$s-none.wav" "out/bench/$s-$n.wav" || eq=NO
    [ "$n" != none ] && rm -f "out/bench/$s-$n.wav"
  done
  echo "$row | $(echo "scale=2; $base / $best" | bc)× | $eq |"
  rm -f "out/bench/$s-none.wav"
done
