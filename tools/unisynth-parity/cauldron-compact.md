# Mini-cauldron: compact vs layered conversion (change compact-cauldron-xm-import)

Generated 2026-10-31 by `bun tools/unisynth-parity/cauldron-compact.ts`. Layered: `synth-convert xm --keep-layers --split-pitches` (the parity layout); compact: the default. Both rendered by `unisynth` (`--profile fast`) and judged against the `openmpt123` reference with the parity metrics (loudness, octave bands, onset alignment). Audio and the listening page stay in `research/tbl-mini-cauldron/out/compact/`.

| song | mode | tracks | layers dropped | song bytes | xz -9e bytes | render s | loudness Δ dB | worst band Δ dB | alignment corr |
|---|---|---|---:|---:|---:|---:|---:|---:|---:|
| jizz | layered | 58 phrase, 13 stored | 0 | 327810 | 17792 | 23.8 | 0.33 | -3.30 @ 16000 | 0.72 |
| jizz | compact | 13 phrase, 11 stored | 591 | 144935 | 14300 | 7.8 | 0.23 | -3.28 @ 16000 | 0.71 |
| stash_song1 | layered | 106 phrase, 13 stored | 0 | 1379893 | 38792 | 68.7 | 0.23 | 0.73 @ 8000 | 0.73 |
| stash_song1 | compact | 35 phrase, 10 stored | 656 | 794825 | 30204 | 29.1 | 0.18 | 1.36 @ 1000 | 0.69 |
| stash_bluish | layered | 64 phrase, 12 stored | 0 | 573993 | 31048 | 26.8 | 0.13 | -2.93 @ 16000 | 0.68 |
| stash_bluish | compact | 29 phrase, 10 stored | 414 | 335155 | 25032 | 10.9 | 0.03 | 1.75 @ 2000 | 0.54 |
