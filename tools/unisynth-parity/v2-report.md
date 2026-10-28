# V2 parity (change add-v2-import)

Generated 2026-10-28 by `bun tools/unisynth-parity/v2.ts`. Reference: v2redux (`v2dump`, the song's own format version); ours: `synth-convert v2m` → `unisynth` (`--profile fast`). Metrics as the reference songs of `report.md`: loudness within 1 dB, octave bands within 3 dB (bands 24 dB under the loudest skipped), onset-strength alignment (lag within 100 frames, correlation ≥ 0.5); renders cut to the shorter one. Songs and audio stay outside the repository (`research/v2-intros.md`).

15 of 15 pass.

| song | ver | result | loudness Δ dB | worst band Δ dB | alignment corr | × real time | notes |
|---|---:|---|---:|---:|---:|---:|---|
| fr08 | 0 | PASS | -0.13 | -0.45 @ 63 | 0.82 | 29.7 |  |
| flybye | 1 | PASS | -0.12 | -0.55 @ 63 | 0.88 | 23.4 |  |
| fr014 | 3 | PASS | 0.05 | 0.79 @ 63 | 0.94 | 34.3 |  |
| fr019 | 4 | PASS | 0.12 | -0.54 @ 1000 | 0.83 | 28.2 |  |
| fr022_final | 5 | PASS | -0.82 | -1.39 @ 1000 | 0.98 | 31.5 |  |
| fr022_party | 3 | PASS | 0.08 | 0.43 @ 2000 | 0.87 | 19.0 |  |
| fr024 | 5 | PASS | -0.24 | -1.34 @ 63 | 0.94 | 25.8 |  |
| fr027 | 5 | PASS | 0.74 | 1.72 @ 16000 | 0.93 | 21.0 |  |
| fr029 | 5 | PASS | 0.15 | 0.41 @ 2000 | 0.92 | 22.5 |  |
| candytron | 5 | PASS | 0.21 | 0.81 @ 1000 | 0.94 | 22.2 |  |
| brullwurfel | 5 | PASS | 0.03 | 0.14 @ 4000 | 0.99 | 37.3 |  |
| farbomat | 3 | PASS | -0.90 | -1.42 @ 250 | 0.88 | 41.2 |  |
| kkrieger | 5 | PASS | -0.20 | 1.07 @ 4000 | 0.84 | 13.3 |  |
| pzero_new | 6 | PASS | 0.11 | 0.30 @ 1000 | 0.96 | 6.6 |  |
| v2_zeitmaschine_new | 6 | PASS | 0.02 | -0.49 @ 125 | 0.97 | 16.1 |  |

## A/B listening

Pending: the acceptance songs are rendered both ways (`--renders`) for the user to compare.
