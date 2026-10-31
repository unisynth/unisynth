# Mini-cauldron: compact vs layered conversion (change compact-cauldron-xm-import)

Generated 2026-10-31 by `bun tools/unisynth-parity/cauldron-compact.ts`. Layered: `synth-convert xm --keep-layers --split-pitches --exact-chain` (the parity layout); dry: `--dry`; compact: the default (dry plus the stereo delay and reverb). Both rendered by `unisynth` (`--profile fast`) and judged against the `openmpt123` reference with the parity metrics (loudness, octave bands, onset alignment). Audio and the listening page stay in `research/tbl-mini-cauldron/out/compact/`.

| song | mode | tracks | layers dropped | song bytes | xz -9e bytes | render s | loudness Δ dB | worst band Δ dB | alignment corr |
|---|---|---|---:|---:|---:|---:|---:|---:|---:|
| jizz | layered | 58 phrase, 13 stored | 0 | 327810 | 17792 | 23.0 | 0.33 | -3.30 @ 16000 | 0.72 |
| jizz | dry | 13 phrase, 11 stored | 591 | 144182 | 14252 | 7.0 | -0.61 | -3.03 @ 16000 | 0.71 |
| jizz | compact | 13 phrase, 11 stored | 591 | 145251 | 14360 | 8.4 | 0.55 | -2.89 @ 16000 | 0.73 |
| stash_song1 | layered | 106 phrase, 13 stored | 0 | 1379893 | 38792 | 70.4 | 0.23 | 0.73 @ 8000 | 0.73 |
| stash_song1 | dry | 35 phrase, 10 stored | 656 | 784170 | 30056 | 23.8 | -0.10 | 0.92 @ 8000 | 0.70 |
| stash_song1 | compact | 35 phrase, 10 stored | 656 | 786006 | 30332 | 28.4 | 0.10 | 1.01 @ 1000 | 0.72 |
| stash_bluish | layered | 64 phrase, 12 stored | 0 | 573993 | 31048 | 27.4 | 0.13 | -2.93 @ 16000 | 0.68 |
| stash_bluish | dry | 29 phrase, 10 stored | 414 | 326174 | 24856 | 8.9 | -0.35 | -1.74 @ 16000 | 0.54 |
| stash_bluish | compact | 29 phrase, 10 stored | 414 | 327549 | 25196 | 12.0 | 0.03 | -1.64 @ 16000 | 0.48 |
