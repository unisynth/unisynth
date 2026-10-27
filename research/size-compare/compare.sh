#!/usr/bin/env bash
# Packed size of the legacy engines (WaveSabre port, Tunefish port) against the unified synth for the same six songs,
# each built under several compile configurations. Uses the three i686 no_std probes (../ws-size-probe, ../tf-size-probe,
# ../us-size-probe); each embeds the song, renders all of it and exits with a checksum, so the sizes are code + song.
#   WS_SONG_DIR=… TF_SONGS=… ES4_SONGS=… ./compare.sh [variant …]   (ES4_SONGS: Enigma-Studio-4/demos/zoom, for turtles)       writes out/results.csv and ../../tools/unisynth-parity/size-compare.md
# Variants (+tags, see build()): z s 3 (precompiled core, panic=abort) and bz bs b3 (-Zbuild-std=core,compiler_builtins with compiler-builtins-mem,
# panic=immediate-abort; bz/bs add optimize_for_size: the compo build's configuration, tools/build-compo.sh).
set -euo pipefail
cd "$(dirname "$0")"
ROOT=$(cd ../.. && pwd); HERE=$PWD
export PATH="$HOME/.cargo/bin:$PATH"
OUT=out; mkdir -p "$OUT/songs" "$OUT/exe"
TOOLS="${TOOLS:-$HOME/Downloads/64k-tools}"
WINE64="${WINE64:-/Applications/Wine Stable.app/Contents/Resources/wine/bin/wine64}"
export WINEDEBUG=-all WINEPREFIX="${WINEPREFIX:-$HOME/.wine-64k}"
VARIANTS=("$@"); [ ${#VARIANTS[@]} -gt 0 ] || VARIANTS=(z s 3 bz bs b3)
BASE="-Clinker-flavor=lld-link -Ctarget-cpu=pentium4 -Crelocation-model=static -Zlocation-detail=none"
for a in /ENTRY:start /SUBSYSTEM:WINDOWS /NODEFAULTLIB /DEBUG:NONE /OPT:REF /OPT:ICF /SAFESEH:NO \
         /BASE:0x400000 /DYNAMICBASE:NO /FIXED; do BASE="$BASE -Clink-arg=$a"; done
sz() { [ -f "$1" ] && stat -f%z "$1" || echo 0; }
sq() { # packer exe-in out
  # squishy under Wine sometimes hangs: 5 minutes per try, one retry
  for _ in 1 2; do
    rm -f "$3"; timeout 300 "$WINE64" "$TOOLS/$1/squishy-x64.exe" -i "$2" -o "$3" >/dev/null 2>&1 || true
    [ -s "$3" ] && break
  done
  sz "$3"
}
CONVERT="$ROOT/target/release/synth-convert"
(cd "$ROOT" && cargo build -q --release -p synth-convert --bin synth-convert && cargo build -q --release -p synth-convert --example legacy_features)
LF="$ROOT/target/release/examples/legacy_features"

# song name | engine | legacy file | legacy features
songs=()
for n in groove:global-groove elysian:elysian engage:engage eladamri:eladamri; do
  f="${WS_SONG_DIR:-}/${n%%:*}.bin"; [ -f "$f" ] || continue
  "$CONVERT" ws "$f" "$OUT/songs/${n##*:}.song" 2>/dev/null
  songs+=("${n##*:}|ws|$f|$("$LF" "$f")")
done
tfmap() { # unified features -> tfsynth features
  echo "$1" | tr ',' '\n' | sed -En -e 's/^us-fx-distortion-power$/tf-fx-distortion/p' -e 's/^us-fx-eq-3band$/tf-fx-eq/p' \
    -e 's/^us-fx-delay-ms$/tf-fx-delay/p' -e 's/^us-fx-(chorus|formant)$/tf-fx-\1/p' -e 's/^us-fx-flanger-ring$/tf-fx-flanger/p' \
    -e 's/^us-fx-reverb-compact$/tf-fx-reverb/p' | paste -sd, -
}
for n in no_remorse:no-remorse tf4testsong:tf4testsong; do
  f="${TF_SONGS:-}/${n%%:*}.tfm"; [ -f "$f" ] || continue
  "$CONVERT" tfm "$f" "$OUT/songs/${n##*:}.song" 2>/dev/null
  songs+=("${n##*:}|tf|$f|$(tfmap "$("$CONVERT" features "$OUT/songs/${n##*:}.song")")")
done
# turtles (2013 Tunefish 4, Enigma Studio 4): legacy = tfsynth playing the final-layout .tfm that `tf4m-tfm` writes
f="${ES4_SONGS:-}/zoom.tf4m"
if [ -f "$f" ]; then
  "$CONVERT" tf4m-tfm "$f" "$OUT/songs/turtles.tfm" 2>/dev/null
  "$CONVERT" tf4m "$f" "$OUT/songs/turtles.song" 2>/dev/null
  songs+=("turtles|tf|$HERE/$OUT/songs/turtles.tfm|$(tfmap "$("$CONVERT" features "$OUT/songs/turtles.song")")")
fi

pre() { [ -n "$2" ] && echo "$2" | tr ',' '\n' | sed "s|^|$1/|" | paste -sd, - || true; }
build() { # probe-dir crate-name variant env… -- features(csv, already prefixed) -> prints exe path
  local dir=$1 crate=$2 var=$3 feats=$4; shift 4
  local flags="$BASE" cargoargs=() opt=z tags=${var#*+} base=${var%%+*}
  [ "$tags" = "$var" ] && tags=""
  # +tags: mf = -Zmerge-functions=aliases, nomir = -Zinline-mir=no
  # (a pentium3 / x87-only build is not possible: core needs sse2 on i686)
  for t in ${tags//+/ }; do case $t in
    mf) flags="$flags -Zmerge-functions=aliases";; nomir) flags="$flags -Zinline-mir=no";;
    *) echo "unknown tag $t" >&2; exit 1;; esac; done
  case $base in
    z|s|3) opt=${base};  flags="$flags -Cpanic=abort";;
    bz|bs|b3) opt=${base#b}
      flags="$flags -Zunstable-options -Cpanic=immediate-abort --cfg builtin_mem"
      local bf=compiler-builtins-mem; [ "$opt" != 3 ] && bf="$bf,optimize_for_size"
      cargoargs=(-Zbuild-std=core,compiler_builtins "-Zbuild-std-features=$bf");;
  esac
  rm -f "$dir/target/i686-pc-windows-msvc/release/$crate.exe"
  (cd "$dir" && env "$@" CARGO_PROFILE_RELEASE_OPT_LEVEL=$opt RUSTFLAGS="$flags" \
     cargo build -q --release ${feats:+--features "$feats"} --target i686-pc-windows-msvc ${cargoargs[@]+"${cargoargs[@]}"} >&2)
  [ -f "$dir/target/i686-pc-windows-msvc/release/$crate.exe" ] || { echo "build failed: $crate $var" >&2; return 1; }
  echo "$dir/target/i686-pc-windows-msvc/release/$crate.exe"
}
csvf="$OUT/results.csv"; [ -f "$csvf" ] || echo "variant,song,engine,song_bytes,song_xz,raw,xz,sq013,features" > "$csvf"
record() { # variant song engine songfile exe features
  local exe="$OUT/exe/$2-$3-$1.exe"; cp "$5" "$exe"; xz -9e -k -c "$exe" > "$exe.xz"
  echo "$1,$2,$3,$(sz "$4"),$(xz -9e -c "$4" | wc -c | tr -d " "),$(sz "$exe"),$(sz "$exe.xz"),$(sq squishy-0.1.3 "$exe" "$exe.sq1"),\"$6\"" >> "$csvf"
  tail -1 "$csvf" >&2
}
for var in "${VARIANTS[@]}"; do
  for s in "${songs[@]}"; do
    IFS='|' read -r name eng file feats <<<"$s"
    grep -q "^$var,$name,legacy," "$csvf" || {
      if [ "$eng" = ws ]; then
        exe=$(build "$HERE/../ws-size-probe" ws-size-probe "$var" "$(pre wavesabre "$feats")" WS_SONG="$file") || exit 1
      else
        exe=$(build "$HERE/../tf-size-probe" tf-size-probe "$var" "$(pre tfsynth "$feats")" TF_SONG="$file") || exit 1
      fi
      record "$var" "$name" legacy "$file" "$exe" "$feats"
    }
    grep -q "^$var,$name,unified," "$csvf" || {
      uf=$("$CONVERT" features "$OUT/songs/$name.song")
      # what a compo build ships: the song as Rust (`synth-convert rust`, probe feature `baked` = `us-trusted`)
      exe=$(build "$HERE/../us-size-probe" us-size-probe "$var" "$(pre unisynth "$uf"),baked" US_SONG="$HERE/$OUT/songs/$name.song") || exit 1
      record "$var" "$name" unified "$OUT/songs/$name.song" "$exe" "$uf,baked"
    }
  done
done

# ---- report
python3 - "$csvf" "$ROOT/tools/unisynth-parity/size-compare.md" <<'PY'
import csv, sys
rows = list(csv.DictReader(open(sys.argv[1])))
d = {(r['variant'], r['song'], r['engine']): r for r in rows}
V = [v for v in dict.fromkeys(r['variant'] for r in rows) if '+' not in v]  # +tag variants: see the note below
S = list(dict.fromkeys(r['song'] for r in rows))
def n(x): return f"{int(x):,}"
def pct(a, b): return f"{(int(b) / int(a) - 1) * 100:+.0f} %"
o = []
o.append("# Legacy engines against the unified synth: packed size, six songs, several compile configurations\n")
o.append("Generated by `research/size-compare/compare.sh`. Each cell is an i686 `no_std` probe (`research/{ws,tf,us}-size-probe`) that embeds the song, "
         "renders all of it and exits with a checksum, so the bytes are the engine code plus the song. Legacy = the WaveSabre port or the Tunefish port with "
         "exactly the devices the original song uses, playing the original `.bin`/`.tfm`; unified = `unisynth` with exactly the features `synth-convert features` "
         "lists, playing the converted song built in as Rust (`synth-convert rust`, `us-trusted`). Packed with squishy 0.1.3 (the compo packer; `xz -9e` in the second table as a packer-independent proxy).\n")
o.append("Configurations (opt-level, then core): `z` `s` `3` use the precompiled core with `panic=abort`; `bz` `bs` `b3` rebuild core "
         "(`-Zbuild-std=core,compiler_builtins`, `compiler-builtins-mem`, `panic=immediate-abort`), `bz` and `bs` with `optimize_for_size` (what `tools/build-compo.sh` "
         "uses). All: LTO, one codegen unit, `-Ctarget-cpu=pentium4`, `-Zlocation-detail=none`.\n")
def table(metric, title):
    o.append(f"## {title}\n")
    o.append("legacy / unified (unified relative to legacy)\n")
    o.append("| song | " + " | ".join(V) + " |")
    o.append("|---|" + "---:|" * len(V))
    for s in S:
        cells = []
        for v in V:
            l, u = d.get((v, s, 'legacy')), d.get((v, s, 'unified'))
            cells.append(f"{n(l[metric])} / {n(u[metric])} ({pct(l[metric], u[metric])})" if l and u else '-')
        o.append(f"| {s} | " + " | ".join(cells) + " |")
    o.append("")
table('sq013', "Squishy 0.1.3 (bytes, packed exe)")
table('xz', "xz -9e (bytes)")
table('raw', "Unpacked exe (bytes)")
o.append("## Songs alone (xz -9e)\n")
o.append("| song | legacy file | legacy xz | converted song | converted xz |\n|---|---:|---:|---:|---:|")
for s in S:
    l, u = d.get((V[0], s, 'legacy')), d.get((V[0], s, 'unified'))
    if l and u: o.append(f"| {s} | {n(l['song_bytes'])} | {n(l['song_xz'])} | {n(u['song_bytes'])} | {n(u['song_xz'])} |")
o.append("")
o.append("## Notes\n")
o.append("- Squishy output comes in 512-byte steps, so differences under 512 B are rounding; `xz` is exact.")
o.append("- Tried and no effect on the unified synth (`bz` plus `-Zmerge-functions=aliases`, plus `-Zinline-mir=no`): byte-identical probes, rows `bz+mf`, `bz+nomir` in `research/size-compare/out/results-v1.csv` (the run before shrink-unified-synth).")
o.append("- Not possible on i686 in this toolchain: `pentium3` or x87-only (`-sse2`): core refuses to build without `sse2` (float ABI).")
open(sys.argv[2], 'w').write("\n".join(o))
PY
