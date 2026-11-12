# Elevated parity (change add-elevated-import)

Generated 2026-11-10 (Atrium, Elevated) and 2026-11-11 (Receptor, change add-receptor-import) by `bun tools/elevated-parity/parity.ts`. Reference: the original synth.asm + music.asm (`build-ref.sh`; Receptor: the intro's own unpacked code, `dump-ref.c`); ours: `synth-convert elevated` → `unisynth` (`--profile fast`). Tolerances as tools/unisynth-parity's song parity: loudness 1 dB, octave bands 3 dB (bands 24 dB under the loudest skipped), timing by onset-strength alignment (correlation ≥ 0.5); peak-picked onsets listed, not judged; renders cut to the shorter one. Songs and audio stay outside the repository (`research/elevated-synth.md`).

32 of 39 pass.

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
| receptor | PASS | 0.22 | 0.86 @ 250 | 0.94 | PASS | 1 converter warning(s) |
| receptor/solo0 | PASS | -0.52 | 3.47 @ 63 | 0.99 |  | 1 converter warning(s) |
| receptor/solo1 | PASS | 0.26 | 5.13 @ 125 | 0.99 |  | 1 converter warning(s) |
| receptor/solo2 | PASS | 0.04 | 0.76 @ 63 | 1.00 |  | 1 converter warning(s) |
| receptor/solo3 | PASS | 0.12 | 0.35 @ 500 | 1.00 |  | 1 converter warning(s) |
| receptor/solo4 | PASS | 0.16 | 1.93 @ 500 | 0.97 |  | 1 converter warning(s) |
| receptor/solo5 | PASS | 0.06 | -1.85 @ 63 | 0.94 |  | 1 converter warning(s) |
| receptor/solo6 | FAIL | 0.45 | 0.96 @ 500 | 0.31 |  | 1 converter warning(s) |
| receptor/solo7 | FAIL | 0.64 | 2.31 @ 250 | 0.48 |  | 1 converter warning(s) |
| receptor/solo8 | FAIL | 0.01 | -1.44 @ 63 | 0.50 |  | 1 converter warning(s) |
| receptor/solo9 | PASS | 0.29 | 1.97 @ 63 | 0.76 |  | 1 converter warning(s) |
| receptor/solo10 | FAIL | 0.48 | 1.49 @ 125 | 0.49 |  | 1 converter warning(s) |
| receptor/solo11 | PASS | 0.10 | 2.05 @ 16000 | 1.00 |  | 1 converter warning(s) |
| receptor/solo12 | PASS | 0.31 | 0.61 @ 1000 | 1.00 |  | 1 converter warning(s) |
| receptor/solo13 | PASS | 0.48 | 1.81 @ 125 | 1.00 |  | 1 converter warning(s) |
| receptor/solo14 | FAIL | 0.47 | 1.04 @ 250 | 0.55 |  | 1 converter warning(s) |
| receptor/solo15 | PASS | 0.26 | 3.01 @ 1000 | 0.64 |  | 1 converter warning(s) |
