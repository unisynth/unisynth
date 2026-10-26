#!/usr/bin/env bash
# Where the bytes go: builds the probe for one song in the compo configuration but without LTO and without inlining
# (so the linker map keeps one symbol per function, legacy-mangled), then sums symbol sizes per module path.
# Sizes are therefore larger than the shipped build's; use them to rank, not to total.
#   ./attr.sh <unified.song> [depth]       features come from `synth-convert features`; depth = path segments to group by (default 3)
set -euo pipefail
cd "$(dirname "$0")"
export PATH="$HOME/.cargo/bin:$PATH"
SONG=$(cd "$(dirname "$1")" && pwd)/$(basename "$1"); DEPTH=${2:-3}
FEATS=$(../../target/release/synth-convert features "$SONG" | tr ',' '\n' | sed 's|^|unisynth/|' | paste -sd, -)
F="-Clinker-flavor=lld-link -Ctarget-cpu=pentium4 -Crelocation-model=static -Zlocation-detail=none -Csymbol-mangling-version=legacy -Zunstable-options -Cpanic=immediate-abort --cfg builtin_mem -Cllvm-args=-inline-threshold=0"
for a in /ENTRY:start /SUBSYSTEM:WINDOWS /NODEFAULTLIB /DEBUG:NONE /OPT:REF /OPT:ICF /SAFESEH:NO /BASE:0x400000 /DYNAMICBASE:NO /FIXED /MAP:$PWD/out/attr.map; do F="$F -Clink-arg=$a"; done
US_SONG="$SONG" RUSTFLAGS="$F" CARGO_PROFILE_RELEASE_OPT_LEVEL=z CARGO_PROFILE_RELEASE_LTO=false CARGO_PROFILE_RELEASE_CODEGEN_UNITS=16 \
  cargo build -q --release ${FEATS:+--features "$FEATS"} --target i686-pc-windows-msvc -Zbuild-std=core,compiler_builtins -Zbuild-std-features=compiler-builtins-mem,optimize_for_size >&2
python3 attr.py out/attr.map "$DEPTH"
