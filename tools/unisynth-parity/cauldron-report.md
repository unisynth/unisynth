# Mini-cauldron parity (change add-cauldron-xm-import)

Generated 2026-10-30 by `bun tools/unisynth-parity/cauldron.ts`. Reference: `openmpt123` (44.1 kHz) of the XMs rebuilt from the bit-exact Python references, and each phrase at the full internal rate cut at its stored Nyquist (the port drops the decimation, so its aliasing is not judged), resampled to 44.1 kHz; ours: `synth-convert xm` / `xm-phrase` → `unisynth` (`--profile fast`). Loudness within 1 dB, octave bands within 3 dB (bands 24 dB under the loudest skipped; phrases judge only bands lying wholly below their stored Nyquist), onset-strength alignment (lag within 100 frames, correlation ≥ 0.5). Audio stays in `research/tbl-mini-cauldron/out`.

## Songs

3 of 3 pass.

| name | result | loudness Δ dB | worst band Δ dB | alignment corr | ping-pong s | notes |
|---|---|---:|---:|---:|---:|---|
| jizz | PASS | 0.33 | -3.30 @ 16000 | 0.72 | 0.00 |  |
| stash_song1 | PASS | 0.23 | 0.73 @ 8000 | 0.72 | 473.50 | 26 converter warning(s) |
| stash_bluish | PASS | 0.13 | -2.93 @ 16000 | 0.68 | 160.60 | 9 converter warning(s) |

## Phrases

82 of 85 pass.

| name | result | loudness Δ dB | worst band Δ dB | alignment corr | notes |
|---|---|---:|---:|---:|---|
| jizz_Probe111 | PASS | -0.06 | 13.29 @ 16000 | 0.93 |  |
| jizz_Probe121 | PASS | -0.08 | 4.58 @ 16000 | 0.99 |  |
| jizz_Probe122 | PASS | -0.09 | -2.59 @ 8000 | 1.00 |  |
| jizz_Probe91 | PASS | -0.13 | 4.66 @ 16000 | 0.99 |  |
| jizz_Probe101 | PASS | -0.02 | -3.49 @ 8000 | 0.98 |  |
| jizz_Probe61 | PASS | 0.09 | 8.84 @ 16000 | 0.97 |  |
| jizz_Probe62 | PASS | -0.03 | -2.34 @ 8000 | 0.99 |  |
| jizz_Probe71 | PASS | 0.25 | 4.37 @ 16000 | 0.99 |  |
| jizz_Probe72 | PASS | 0.16 | 12.43 @ 16000 | 0.96 |  |
| jizz_Probe41 | PASS | -0.12 | 10.80 @ 16000 | 0.93 |  |
| jizz_Probe42 | PASS | -0.19 | 5.70 @ 16000 | 0.96 |  |
| jizz_Probe43 | PASS | -0.17 | 2.63 @ 16000 | 0.98 |  |
| jizz_Probe44 | PASS | -0.19 | -3.63 @ 8000 | 0.97 |  |
| jizz_Probe81 | PASS | 0.56 | 22.56 @ 16000 | 0.98 |  |
| jizz_Probe11 | PASS | -0.07 | 15.44 @ 16000 | 0.91 |  |
| jizz_Probe12 | PASS | -0.03 | 11.77 @ 16000 | 0.98 |  |
| jizz_Probe13 | PASS | -0.10 | 7.17 @ 16000 | 0.97 |  |
| jizz_Probe14 | PASS | -0.18 | 2.67 @ 16000 | 0.93 |  |
| jizz_Probe31 | PASS | -0.06 | 14.18 @ 16000 | 0.89 |  |
| jizz_Probe32 | PASS | 0.02 | 11.02 @ 16000 | 0.97 |  |
| jizz_Probe33 | PASS | -0.08 | 5.02 @ 16000 | 0.99 |  |
| jizz_Probe34 | PASS | -0.16 | -2.56 @ 8000 | 0.97 |  |
| jizz_Probe51 | PASS | 0.09 | 9.17 @ 16000 | 0.96 |  |
| jizz_Probe52 | PASS | 0.11 | 7.22 @ 16000 | 0.97 |  |
| jizz_Probe53 | PASS | 0.14 | 6.28 @ 16000 | 0.96 |  |
| jizz_Probe54 | PASS | 0.14 | 4.09 @ 16000 | 0.96 |  |
| jizz_Probe55 | PASS | 0.20 | -2.77 @ 8000 | 0.96 |  |
| jizz_Probe56 | PASS | 0.17 | -3.39 @ 8000 | 0.96 |  |
| jizz_Probe57 | PASS | 0.13 | -3.18 @ 8000 | 0.95 |  |
| jizz_Probe58 | PASS | 0.02 | -2.85 @ 8000 | 0.94 |  |
| jizz_Probe21 | PASS | -0.48 | 16.57 @ 16000 | 0.91 |  |
| jizz_Probe22 | PASS | -0.01 | 9.61 @ 16000 | 0.93 |  |
| stash_bluish_0_0_Jace11 | PASS | -0.06 | 25.78 @ 16000 | 0.95 |  |
| stash_bluish_0_1_Jace12 | PASS | -0.07 | 30.15 @ 16000 | 0.94 |  |
| stash_bluish_0_2_Jace13 | PASS | -0.07 | 6.32 @ 16000 | 0.92 |  |
| stash_bluish_0_3_Jace14 | PASS | -0.35 | 19.16 @ 16000 | 0.93 |  |
| stash_bluish_2_0_Jace31 | PASS | 0.24 | 14.80 @ 16000 | 0.96 |  |
| stash_bluish_2_1_Jace32 | PASS | 0.14 | 6.95 @ 16000 | 0.96 |  |
| stash_bluish_2_2_Jace33 | PASS | 0.22 | 4.52 @ 16000 | 0.96 |  |
| stash_bluish_2_3_Jace34 | PASS | 0.17 | -3.52 @ 8000 | 0.96 |  |
| stash_bluish_3_0_Jace41 | PASS | 0.45 | 5.42 @ 16000 | 1.00 |  |
| stash_bluish_4_0_Jace51 | PASS | -0.29 | -4.73 @ 16000 | 0.83 |  |
| stash_bluish_5_0_Jace61 | PASS | -0.07 | -1.82 @ 125 | 1.00 |  |
| stash_bluish_6_0_Jace71 | PASS | -0.83 | -4.92 @ 8000 | 0.91 |  |
| stash_bluish_7_0_Jace81 | PASS | -0.06 | -10.23 @ 16000 | 0.99 |  |
| stash_bluish_8_0_Jace91 | PASS | -0.25 | -2.70 @ 16000 | 0.66 |  |
| stash_bluish_8_1_Jace92 | PASS | -0.19 | -2.01 @ 16000 | 0.59 |  |
| stash_bluish_8_2_Jace93 | PASS | -0.22 | -2.45 @ 16000 | 0.54 |  |
| stash_bluish_8_3_Jace94 | PASS | -0.71 | -2.98 @ 16000 | 0.52 |  |
| stash_bluish_9_0_Jacea1 | PASS | -0.02 | 8.82 @ 16000 | 0.84 |  |
| stash_bluish_9_1_Jacea2 | PASS | -0.23 | 2.38 @ 16000 | 0.72 |  |
| stash_bluish_9_2_Jacea3 | PASS | 0.12 | -0.68 @ 8000 | 0.50 |  |
| stash_bluish_9_3_Jacea4 | PASS | 0.03 | -0.93 @ 8000 | 0.77 |  |
| stash_bluish_10_0_Jaceb1 | PASS | -0.18 | -4.35 @ 16000 | 1.00 |  |
| stash_song0_0_0_Jace11 | PASS | -0.02 | 17.95 @ 16000 | 0.92 |  |
| stash_song0_0_1_Jace12 | PASS | 0.08 | 21.92 @ 16000 | 0.90 |  |
| stash_song0_0_2_Jace13 | PASS | -0.01 | 15.36 @ 16000 | 0.95 |  |
| stash_song0_0_3_Jace14 | PASS | -1.00 | -3.47 @ 8000 | 0.90 |  |
| stash_song0_2_0_Jace21 | PASS | 0.14 | -1.16 @ 16000 | 1.00 |  |
| stash_song0_3_0_Jace31 | PASS | 0.21 | 11.08 @ 16000 | 0.96 |  |
| stash_song0_4_0_Jace41 | FAIL | -0.03 | -9.95 @ 4000 | 0.98 |  |
| stash_song0_5_0_Jace51 | FAIL | -2.28 | 16.19 @ 16000 | 0.94 |  |
| stash_song0_6_0_Jace61 | PASS | -0.09 | -1.46 @ 125 | 0.95 |  |
| stash_song0_7_0_Jace71 | PASS | -0.24 | 3.99 @ 16000 | 0.99 |  |
| stash_song0_8_0_Jace81 | PASS | -0.05 | -2.89 @ 16000 | 0.99 |  |
| stash_song0_9_0_Jace91 | PASS | -0.06 | 1.68 @ 16000 | 0.77 |  |
| stash_song0_10_0_Jacea1 | PASS | -0.29 | 11.24 @ 8000 | 0.89 |  |
| stash_song0_11_0_Jaceb1 | FAIL | 1.11 | 53.01 @ 8000 | 0.96 |  |
| stash_song0_12_0_Jacec1 | PASS | 0.02 | 25.53 @ 8000 | 0.93 |  |
| stash_song0_12_1_Jacec2 | PASS | 0.17 | 12.52 @ 8000 | 0.89 |  |
| stash_song0_12_2_Jacec3 | PASS | 0.56 | 9.66 @ 8000 | 0.91 |  |
| stash_song0_12_3_Jacec4 | PASS | 0.33 | 9.77 @ 8000 | 0.88 |  |
| stash_song0_13_0_Jaced1 | PASS | -0.19 | 20.91 @ 16000 | 0.94 |  |
| stash_song0_13_1_Jaced2 | PASS | -0.30 | 28.29 @ 16000 | 0.89 |  |
| stash_song0_13_2_Jaced3 | PASS | -0.27 | 15.55 @ 16000 | 0.85 |  |
| stash_song0_13_3_Jaced4 | PASS | -0.47 | 11.85 @ 16000 | 0.82 |  |
| stash_song0_14_0_Jacee1 | PASS | -0.02 | 35.51 @ 16000 | 0.91 |  |
| stash_song0_14_1_Jacee2 | PASS | -0.01 | 13.23 @ 16000 | 0.94 |  |
| stash_song0_14_2_Jacee3 | PASS | 0.03 | 5.78 @ 16000 | 0.93 |  |
| stash_song0_14_3_Jacee4 | PASS | 0.71 | 4.78 @ 16000 | 0.86 |  |
| stash_song0_15_0_Jacef1 | PASS | 0.65 | 34.13 @ 16000 | 0.99 |  |
| stash_song0_16_0_Jaceg1 | PASS | 0.04 | 15.67 @ 8000 | 0.89 |  |
| stash_song0_16_1_Jaceg2 | PASS | 0.26 | 12.03 @ 8000 | 0.84 |  |
| stash_song0_16_2_Jaceg3 | PASS | -0.36 | 9.27 @ 8000 | 0.89 |  |
| stash_song0_16_3_Jaceg4 | PASS | -0.06 | 27.91 @ 8000 | 0.87 |  |

## A/B listening

Pending: the user compares `out/listen/*.mp3` (reference) with `out/songs/*.mp3` (converted).
