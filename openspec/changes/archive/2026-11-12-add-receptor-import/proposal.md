# Proposal

## Why

**Receptor** (TBC, NVScene 2008 4k, 2nd; demozoo 31385, pouet 51449; code Mentor, music Puryx) plays on Mentor's 4k
synth, the Atrium generation, but no source was released: only the Crinkler-packed exes. Unpacked, its machine table
(8 machines), delay, filter, compressor, mixer, both distortions, allpass, oscillator and noise are instruction-for-
instruction Atrium's. Only the synth machine differs:
- **Glide:** bit 7 on a note byte slides its pitch linearly in frequency towards the next non-empty byte of the same
  16-step pattern (searched cyclically), at the rate that reaches it on that step. The slide runs until the note is
  overwritten.
- **Cut:** `0x7F` and `0xFF` render the note with a zero gate (Atrium: a zero envelope). Same output.
- Per-row note counters for the visuals (no audio effect).

User decision (2026-11-11): carve the song from the unpacked exe, add the glide to the converter, and use the intro's
own code as the reference.

## What Changes

- `tools/elevated-parity/crinkler-unpack.py` (unicorn): dumps the unpacked image of a Crinkler exe.
- `tools/elevated-parity/carve.py`: writes `music.asm`/`music.h` from the dump at the build's addresses, with checks.
- `tools/elevated-parity/dump-ref.c` (MIT): maps the dump at its address inside the harness's own `.bss` (image base
  0x400000), runs the intro's `generateMusic` with the x87 at 24 bits (the intro calls it after
  `IDirect3D9::CreateDevice` without `FPU_PRESERVE`), writes the WAV; `--solo k`.
- `build-ref.sh receptor`, `parity.ts receptor`, `captures.sh` (`0UOpWfX6gw8`).
- `synth-convert elevated`: `0xFF` cuts, `0x80` is empty, a flagged note plays its note and its glide becomes a
  pitch-bend lane (linear in frequency, within 0.05 semitones of the curve) with the track's bend range set; a glide
  past ±24 semitones plays its note shifted so both ends fit.

No engine change: the bend controller and automation exist.

## Discipline Skills

`systematic-debugging` (parity failures), `doubt-driven-review` (reference built from the intro's own binary).
