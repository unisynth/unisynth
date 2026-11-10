# Elevated parity (change add-elevated-import)

Generated 2026-11-10 by `bun tools/elevated-parity/parity.ts`. Reference: the original synth.asm + music.asm (`build-ref.sh`); ours: `synth-convert elevated` → `unisynth` (`--profile fast`). Tolerances as tools/unisynth-parity's song parity: loudness 1 dB, octave bands 3 dB (bands 24 dB under the loudest skipped), timing by onset-strength alignment (correlation ≥ 0.5); peak-picked onsets listed, not judged; renders cut to the shorter one. Songs and audio stay outside the repository (`research/elevated-synth.md`).

20 of 22 pass.

| render | result | loudness Δ dB | worst band Δ dB | alignment corr | capture | notes |
|---|---|---:|---:|---|---|---|
| atrium | PASS | 0.14 | 1.08 @ 1000 | 0.99 | PASS |  |
| atrium/solo0 | PASS | 0.00 | -2.28 @ 250 | 0.93 |  |  |
| atrium/solo1 | FAIL | 0.23 | 1.48 @ 500 | 0.32 |  |  |
| atrium/solo2 | PASS | -0.28 | 3.78 @ 63 | 1.00 |  |  |
| atrium/solo3 | PASS | 0.18 | 0.56 @ 4000 | 0.98 |  |  |
| atrium/solo4 | PASS | 0.06 | 1.49 @ 63 | 0.93 |  |  |
| atrium/solo5 | PASS | 0.02 | 2.30 @ 63 | 0.94 |  |  |
| atrium/solo6 | PASS | 1.00 | 1.45 @ 1000 | 0.98 |  |  |
| atrium/solo7 | PASS | 0.04 | -2.11 @ 16000 | 1.00 |  |  |
| elevated | PASS | -0.03 | -0.51 @ 125 | 0.87 | PASS |  |
| elevated/solo0 | PASS | -0.21 | 1.18 @ 63 | 1.00 |  |  |
| elevated/solo1 | PASS | 0.02 | 3.10 @ 63 | 0.93 |  |  |
| elevated/solo2 | PASS | 0.08 | -1.14 @ 125 | 0.94 |  |  |
| elevated/solo3 | PASS | 0.07 | -1.14 @ 125 | 0.93 |  |  |
| elevated/solo4 | PASS | 0.02 | -1.17 @ 8000 | 1.00 |  |  |
| elevated/solo5 | PASS | -0.33 | -0.78 @ 125 | 0.56 |  |  |
| elevated/solo6 | FAIL | 0.14 | 1.01 @ 250 | 0.35 |  |  |
| elevated/solo7 | PASS | 0.28 | 1.16 @ 125 | 0.56 |  |  |
| elevated/solo8 | PASS | 0.04 | 1.49 @ 63 | 0.70 |  |  |
| elevated/solo9 | PASS | 0.02 | -0.84 @ 63 | 0.58 |  |  |
| elevated/solo10 | PASS | 0.01 | 1.40 @ 63 | 0.74 |  |  |
| elevated/solo11 | PASS | 0.23 | 0.32 @ 4000 | 0.56 |  |  |
