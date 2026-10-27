#!/usr/bin/env bash
# Bit-exactness of baked songs: renders each song built from its blob (every unisynth module, Song::parse, full load
# checks) and from `synth-convert rust` (exactly its features + `us-trusted`), and requires identical hashes.
#   ./check.sh [song.song …]      default: ../us-size-probe/out/songs/{the seven reference songs}.song
set -uo pipefail
cd "$(dirname "$0")"
export PATH="$HOME/.cargo/bin:$PATH"
CONVERT=../../target/release/synth-convert
(cd ../.. && cargo build -q --release -p synth-convert --bin synth-convert)
SONGS=("$@")
[ ${#SONGS[@]} -gt 0 ] || for s in global-groove elysian engage eladamri no-remorse tf4testsong turtles; do SONGS+=("../us-size-probe/out/songs/$s.song"); done
fail=0
for s in "${SONGS[@]}"; do
  s=$(cd "$(dirname "$s")" && pwd)/$(basename "$s")
  blob=$(US_SONG="$s" cargo run -q --release --features unisynth/us-all 2>&1) || { echo "FAIL  $s (blob build)"; echo "$blob" | tail -5; fail=1; continue; }
  feats=$("$CONVERT" features "$s" | tr ',' '\n' | grep . | sed 's|^|unisynth/|' | paste -sd, -)
  baked=$(US_SONG="$s" cargo run -q --release --features "baked${feats:+,$feats}" 2>&1) || { echo "FAIL  $s (baked build)"; echo "$baked" | tail -5; fail=1; continue; }
  if [ "$blob" = "$baked" ]; then echo "ok    $(basename "$s")"; else echo "FAIL  $(basename "$s")"; diff <(echo "$blob") <(echo "$baked"); fail=1; fi
done
exit $fail
