#!/usr/bin/env bash
# End-to-end: fetch the official iXalance containers, unpack, run the intros
# under unicorn until the music starts (oracle XMs), extract patch data,
# render with the Python reference and compare byte for byte.
# Needs: python3 >= 3.13 (math.fma), curl; venv with unicorn + capstone + numpy is created in out/venv.
set -euo pipefail
cd "$(dirname "$0")/.."
OUT=out
mkdir -p "$OUT"
[ -x "$OUT/venv/bin/python" ] || { python3 -m venv "$OUT/venv"; "$OUT/venv/bin/pip" -q install unicorn capstone numpy; }
PY="$OUT/venv/bin/python"
for d in jizz stash; do
  [ -f "$OUT/$d.ixa" ] || curl -sSfL -o "$OUT/$d.ixa" "https://www.libsdl.org/projects/ixalance/data/$d.ixa"
  "$PY" tools/unixa.py "$OUT/$d.ixa" "$OUT/$d"
  ls "$OUT/$d"/xm_*.xm >/dev/null 2>&1 || "$PY" tools/ixa_host.py "$OUT/$d"   # ~25 s each
done
# Optional cross-check of the emulator against the official rip of jizz.xm (scene.org demodulate):
if [ ! -f "$OUT/jizz_rip.zip" ]; then curl -sSfL -o "$OUT/jizz_rip.zip" https://files.scene.org/get/demos/compilations/demodulate/jizz.zip || true; fi
if [ -f "$OUT/jizz_rip.zip" ]; then unzip -qo "$OUT/jizz_rip.zip" jizz.xm -d "$OUT" && cmp "$OUT/jizz.xm" "$OUT/jizz/xm_41a6e4.xm" && echo "emulated jizz.xm == official rip"; fi
"$PY" tools/extract_patches.py "$OUT"
( cd ref && python3 mcauldron1.py patches_jizz.json "../$OUT/render_jizz" >/dev/null )
( cd ref && python3 mcauldron2.py patches_stash.json "../$OUT/render_stash" >/dev/null )
python3 tools/verify.py jizz  "$OUT/jizz/xm_41a6e4.xm" "$OUT/render_jizz"
python3 tools/verify.py stash "$OUT/stash/xm_5389b8.xm" "$OUT/stash/xm_45cf38.xm" "$OUT/render_stash" "$OUT/stash/drums.json"
# Listenable output: XMs rebuilt from the reference renders + per-phrase WAVs (needs openmpt123 + ffmpeg for the mp3s)
"$PY" tools/rebuild_xm.py "$OUT"
if command -v openmpt123 >/dev/null && command -v ffmpeg >/dev/null; then
  ( cd "$OUT/listen" && for f in jizz stash_song1 stash_bluish; do
      openmpt123 --quiet --batch --render --force --samplerate 44100 "$f.xm" >/dev/null && ffmpeg -v error -y -i "$f.xm.wav" -b:a 192k "$f.mp3"
      mkdir -p ref44 && mv "$f.xm.wav" "ref44/$f.wav"; done )   # ref44/<song>.wav = parity reference
fi
# Per-phrase mp3s (index.html prefers them over the wavs)
if command -v ffmpeg >/dev/null; then
  for w in "$OUT"/listen/phrases/*.wav; do ffmpeg -v error -y -i "$w" -ar 44100 -b:a 192k "${w%.wav}.mp3"; done
  "$PY" tools/rebuild_xm.py "$OUT" >/dev/null
fi
# Alias-free phrase references for the parity suite (full rate, cut at the stored Nyquist; needs numpy, ~1 min)
"$OUT/venv/bin/python" -c "import numpy" 2>/dev/null || "$OUT/venv/bin/pip" -q install numpy
"$PY" tools/fullrate_phrases.py "$OUT" >/dev/null
# Converted songs (change add-cauldron-xm-import): when synth-convert and unisynth's render example are built
# (cargo build --profile fast -p synth-convert --bin synth-convert && cargo build --profile fast -p unisynth
# --features std --example render), convert the three XMs into out/songs/ and render them next to the references
# as <song>.unisynth.mp3 (lead-in trimmed); ~3 min of rendering.
ROOT="$(cd ../.. && pwd)"
for prof in fast release; do
  SC="$ROOT/target/$prof/synth-convert"; RD="$ROOT/target/$prof/examples/render"
  [ -x "$SC" ] && [ -x "$RD" ] && break
done
if [ -x "$SC" ] && [ -x "$RD" ] && command -v ffmpeg >/dev/null; then
  mkdir -p "$OUT/songs"
  for f in jizz:patches_jizz stash_song1:patches_stash stash_bluish:patches_stash; do
    n=${f%%:*}; pj=${f##*:}
    "$SC" xm "$OUT/listen/$n.xm" "ref/$pj.json" "$OUT/songs/$n.song" 2>"$OUT/songs/$n.log"
    "$RD" "$OUT/songs/$n.song" "$OUT/songs/$n.wav" >/dev/null
    lead=$(sed -n 's/^lead-frames //p' "$OUT/songs/$n.log")
    ffmpeg -v error -y -i "$OUT/songs/$n.wav" -af "atrim=start_sample=$lead" -b:a 192k "$OUT/listen/$n.unisynth.mp3"
  done
  python3 -c "import sys; sys.path.insert(0, 'tools'); import rebuild_xm; rebuild_xm.write_index('$OUT/listen')"
  echo "converted songs: $OUT/listen/*.unisynth.mp3"
fi
