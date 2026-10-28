# V2 parity (change add-v2-import)

Generated 2026-10-28 by `bun tools/unisynth-parity/v2.ts`. Reference: v2redux (`v2dump`, the song's own format version); ours: `synth-convert v2m` → `unisynth` (`--profile fast`). Metrics as the reference songs of `report.md`: loudness within 1 dB, octave bands within 3 dB (bands 24 dB under the loudest skipped), onset-strength alignment (lag within 100 frames, correlation ≥ 0.5); renders cut to the shorter one. Songs and audio stay outside the repository (`research/v2-intros.md`).

15 of 15 pass.

| song | ver | result | loudness Δ dB | worst band Δ dB | alignment corr | × real time | notes |
|---|---:|---|---:|---:|---:|---:|---|
| fr08 | 0 | PASS | -0.13 | -0.45 @ 63 | 0.82 | 27.4 |  |
| flybye | 1 | PASS | -0.12 | -0.55 @ 63 | 0.88 | 21.6 |  |
| fr014 | 3 | PASS | 0.05 | 0.79 @ 63 | 0.94 | 31.6 |  |
| fr019 | 4 | PASS | 0.12 | -0.54 @ 1000 | 0.83 | 26.7 |  |
| fr022_final | 5 | PASS | -0.82 | -1.39 @ 1000 | 0.98 | 29.6 |  |
| fr022_party | 3 | PASS | 0.08 | 0.43 @ 2000 | 0.87 | 17.7 |  |
| fr024 | 5 | PASS | -0.24 | -1.34 @ 63 | 0.94 | 24.0 |  |
| fr027 | 5 | PASS | 0.74 | 1.72 @ 16000 | 0.93 | 19.5 |  |
| fr029 | 5 | PASS | 0.15 | 0.41 @ 2000 | 0.92 | 21.0 |  |
| candytron | 5 | PASS | 0.21 | 0.81 @ 1000 | 0.94 | 20.6 |  |
| brullwurfel | 5 | PASS | 0.03 | 0.14 @ 4000 | 0.99 | 34.5 |  |
| farbomat | 3 | PASS | -0.90 | -1.42 @ 250 | 0.88 | 38.0 |  |
| kkrieger | 5 | PASS | -0.20 | 1.07 @ 4000 | 0.84 | 12.4 |  |
| pzero_new | 6 | PASS | 0.11 | 0.30 @ 1000 | 0.96 | 6.2 |  |
| v2_zeitmaschine_new | 6 | PASS | 0.02 | -0.49 @ 125 | 0.97 | 14.9 |  |

## Aux buses (corpus songs)

Songs outside the acceptance set that use V2's aux A/B buses (aux oscillators, aux receives).

3 of 3 pass.

| song | ver | result | loudness Δ dB | worst band Δ dB | alignment corr | × real time | notes |
|---|---:|---|---:|---:|---:|---:|---|
| debris_ost | 6 | PASS | -0.10 | -0.72 @ 500 | 0.72 | 12.5 |  |
| fr-052_platinum | 6 | PASS | -0.07 | -1.81 @ 500 | 0.91 | 15.0 |  |
| stars | 6 | PASS | 0.42 | 1.26 @ 250 | 0.79 | 36.9 |  |

## A/B listening

Pending: the acceptance songs are rendered both ways (`--renders`) for the user to compare.
