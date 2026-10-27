#!/usr/bin/env bash
# add-synth-threads size gate: builds the i686 no_std size probes WITHOUT the `threads` feature and prints the
# SHA-256 of each executable (/Brepro: no link timestamp, so equal code gives equal bytes). Run before the change (→ baseline.txt) and after; the lists must be identical.
#   ./hashes.sh > now.txt && diff baseline.txt now.txt
set -euo pipefail
cd "$(dirname "$0")/.."
ROOT=$(cd .. && pwd)
export PATH="$HOME/.cargo/bin:$PATH"
F="-Clinker-flavor=lld-link -Ctarget-cpu=pentium4 -Crelocation-model=static -Zlocation-detail=none"
for a in /ENTRY:start /SUBSYSTEM:WINDOWS /NODEFAULTLIB /DEBUG:NONE /OPT:REF /OPT:ICF /SAFESEH:NO \
         /BASE:0x400000 /DYNAMICBASE:NO /FIXED /Brepro; do F="$F -Clink-arg=$a"; done
h() { shasum -a 256 "$1" | cut -d' ' -f1; }
for feat in all slaughter; do
  (cd ws-size-probe && RUSTFLAGS="$F" cargo build -q --release --features "$feat" --target i686-pc-windows-msvc)
  echo "ws-$feat $(h ws-size-probe/target/i686-pc-windows-msvc/release/ws-size-probe.exe)"
done
# unified synth: converted WaveSabre showcase song, every module
mkdir -p size-threads/out
(cd "$ROOT" && FEATURES_DUMP="$PWD/research/size-threads/out" cargo test -q -p synth-convert --test features \
  dump_songs_for_the_compo_build_check >/dev/null)
(cd us-size-probe && US_SONG="$ROOT/research/size-threads/out/showcase.song" RUSTFLAGS="$F" \
  cargo build -q --release --features unisynth/us-all --target i686-pc-windows-msvc)
echo "us-all $(h us-size-probe/target/i686-pc-windows-msvc/release/us-size-probe.exe)"
# unified synth per project song, with exactly the modules `synth-convert features` lists
(cd "$ROOT" && cargo build -q --release -p song -p synth-convert)
for s in "$ROOT"/project/music/*.song.json; do
  n=$(basename "$s" .song.json)
  "$ROOT/target/release/song" export "$s" -o "size-threads/out/$n.bin"
  fl=$("$ROOT/target/release/synth-convert" features "size-threads/out/$n.bin" | tr ',' '\n' | sed 's|^|unisynth/|' | paste -sd, -)
  (cd us-size-probe && US_SONG="$ROOT/research/size-threads/out/$n.bin" RUSTFLAGS="$F" \
    cargo build -q --release ${fl:+--features "$fl"} --target i686-pc-windows-msvc)
  echo "us-$n $(h us-size-probe/target/i686-pc-windows-msvc/release/us-size-probe.exe)"
done
