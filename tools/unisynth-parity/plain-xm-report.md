# Plain-XM parity (1998 softsynth intros)

Generated 2026-11-10 by `bun tools/unisynth-parity/plain-xm.ts`. Reference: `openmpt123` (44.1 kHz) of each intro's XM with every generated sample as the build its generator map was extracted from generated it (`tools/xm-gen`); ours: `synth-convert xm <xm> <map> --keep-layers --split-pitches --exact-chain` → `unisynth` (`--profile fast`). Loudness within 1 dB, octave bands within 3 dB, onset alignment. "Received XM" compares the XM as received with the reference, for reading. Audio stays in `$ZENEKK_DIR/xm-gen/parity`.

| Intro | Status | Ours vs reference | Received XM vs reference | Note |
|---|---|---|---|---|
| fudge-asc | pending | – | -0.00 dB, bands ≤ 0.00 dB, within tolerance | engine `fm-shaper` not in this build yet |
| quad-theta | pending | – | -0.06 dB, bands ≤ 2.06 dB, within tolerance | engine `poespack` not in this build yet |
| quad-pure | pending | – | 0.00 dB, bands ≤ 0.00 dB, within tolerance | engine `poespack` not in this build yet |
