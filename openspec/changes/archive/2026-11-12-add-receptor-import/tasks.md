# Tasks

## 1. Reference
- [x] 1.1 `crinkler-unpack.py`: emulate the decompressor, dump at the jump into the unpacked code
- [x] 1.2 compare the unpacked synth with Atrium's (machines 1–7, osc, noise identical; synth machine: glide, gate cut)
- [x] 1.3 `carve.py receptor`: music.asm/music.h; tree walks under table 8, step and rows checked against the code
- [x] 1.4 `dump-ref.c` + `build-ref.sh receptor` (SOLO=1); x87 at 24 bits
- [x] 1.5 capture `0UOpWfX6gw8`: the reference render passes the capture check (24- and 53-bit alike)

## 2. Converter
- [x] 2.1 tests: `0xFF` cut, `0x80` empty, glide target cyclic in its pattern, glide linear in Hz, glide past 24 semitones
- [x] 2.2 `glides`, `bend_plan`, bend lane + bend range in `convert`

## 3. Acceptance
- [x] 3.1 `parity.ts --solo --capture receptor` passes; table in `research/elevated-synth.md` §12
- [x] 3.3 `make-demos.py` writes `project/music/released/receptor.song.json`; `project/music/NOTICE.md` entry (no licence, permission not yet asked)
- [x] 3.2 docs: `docs/synth/elevated.md`, `tools/AGENTS.md`, `crates/synth-convert` records

## 4. Found in acceptance
- [x] 4.1 glide lane anchored at its note (the ramp bent the notes before every glide; solo 5 −5.8 → −1.85 dB @ 63 Hz); test `notes_before_a_glide_keep_their_pitch`
