# Mini-cauldron: compact vs layered conversion (change compact-cauldron-xm-import)

Generated 2026-10-31 by `bun tools/unisynth-parity/cauldron-compact.ts`. Layered: `synth-convert xm --keep-layers --split-pitches` (the parity layout); dry: `--dry`; compact: the default (dry plus the stereo delay and reverb). Both rendered by `unisynth` (`--profile fast`) and judged against the `openmpt123` reference with the parity metrics (loudness, octave bands, onset alignment). Audio and the listening page stay in `research/tbl-mini-cauldron/out/compact/`.

| song | mode | tracks | layers dropped | song bytes | xz -9e bytes | render s | loudness Δ dB | worst band Δ dB | alignment corr |
|---|---|---|---:|---:|---:|---:|---:|---:|---:|
| jizz | layered | 58 phrase, 13 stored | 0 | 327810 | 17792 | 23.5 | 0.33 | -3.30 @ 16000 | 0.72 |
| jizz | dry | 13 phrase, 11 stored | 591 | 144935 | 14280 | 7.7 | -0.58 | -3.35 @ 16000 | 0.72 |
| jizz | compact | 13 phrase, 11 stored | 591 | 146004 | 14408 | 9.8 | 0.58 | -3.23 @ 16000 | 0.72 |
| stash_song1 | layered | 106 phrase, 13 stored | 0 | 1379893 | 38792 | 70.6 | 0.23 | 0.73 @ 8000 | 0.73 |
| stash_song1 | dry | 35 phrase, 10 stored | 656 | 794645 | 30108 | 29.8 | -0.05 | 1.01 @ 1000 | 0.69 |
| stash_song1 | compact | 35 phrase, 10 stored | 656 | 796481 | 30344 | 35.3 | 0.17 | 1.24 @ 1000 | 0.70 |
| stash_bluish | layered | 64 phrase, 12 stored | 0 | 573993 | 31048 | 28.7 | 0.13 | -2.93 @ 16000 | 0.68 |
| stash_bluish | dry | 29 phrase, 10 stored | 414 | 335155 | 25060 | 12.6 | -0.36 | -2.13 @ 16000 | 0.54 |
| stash_bluish | compact | 29 phrase, 10 stored | 414 | 336530 | 25328 | 14.6 | 0.03 | -2.03 @ 16000 | 0.48 |
