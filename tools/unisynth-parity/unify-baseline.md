# unify-v2-algorithms baseline (task 1.1)

Recorded 2026-10-30 on a clean worktree of `28913c6` (no uncommitted edits), `--profile fast`, before any change of
unify-v2-algorithms. Renders kept outside the repo as the drift reference (`--drift`, design D7.3):
`~/Downloads/64k-tools/synth-lab/unify/base/{ws,v2}/<item>.wav`.

Re-check a later tree:

```bash
B=~/Downloads/64k-tools/synth-lab/unify/base
WS_SONG_DIR=~/Downloads/64k-tools/synth-lab/ws TF_SONGS=~/Downloads/64k-tools/synth-lab/tunefish/media/tf4modules \
  bun tools/unisynth-parity/parity.ts --drift $B/ws --report /tmp/report.md
bun tools/unisynth-parity/v2.ts --drift $B/v2 --report /tmp/v2-report.md
```

## Parity

- `parity.ts`: 49 of 49 items pass (43 device items + 6 reference songs; report `report.md` at that commit).
- `v2.ts`: 18 of 18 pass (15 acceptance + debris_ost, fr-052_platinum, stars; `v2-report.md`).

## Self-goldens

- `crates/unisynth/tests/golden.txt` sha256 `3249174…eabb7`
- `crates/unisynth/tests/golden_fx.txt` sha256 `d11878b…62f8`

## Bake check

`research/us-bake-check/check.sh`: 7 of 7 songs ok.

## Size (compo i686 probe, baked; raw / xz -9e / squishy 0.1.3 bytes; `research/us-size-probe/songs-size.sh`)

| song | raw | xz | squishy |
|---|---:|---:|---:|
| eladamri | 92672 | 38724 | 31744 |
| elysian | 122368 | 53448 | 45568 |
| engage | 151040 | 47684 | 38400 |
| global-groove | 30720 | 15800 | 14336 |
| no-remorse | 103936 | 27620 | 23040 |
| tf4testsong | 58880 | 25772 | 21504 |
| turtles | 144384 | 36168 | 28160 |
| v2-brullwurfel | 109568 | 35428 | 28160 |
| v2-candytron | 157696 | 46512 | 35840 |
| v2-debris | 509440 | 51616 | 38912 |
| v2-fr08 | 354816 | 48488 | 35328 |
| v2-kkrieger | 115200 | 40608 | 32256 |
| v2-platinum | 162304 | 40032 | 31232 |

## Speed

Unified render time of the reference songs (`--profile fast`, seconds; the legacy engine for comparison), and the V2
songs' × real time from `v2-report.md`. Noisy (±10 %) on a shared machine: gate D7.7 compares medians of repeated runs
when a song moves.

| song | unified s | legacy s |
|---|---:|---:|
| global-groove | 0.0177 | 0.0122 |
| elysian | 26.6082 | 32.3118 |
| engage | 36.9473 | 40.1156 |
| eladamri | 9.7104 | 15.4131 |
| no-remorse | 26.9413 | 14.5553 |
| tf4testsong | 2.7128 | 1.7736 |

| V2 song | × real time |
|---|---:|
| fr08 | 22.0 |
| flybye | 10.4 |
| fr014 | 16.1 |
| fr019 | 17.0 |
| fr022_final | 20.2 |
| fr022_party | 12.5 |
| fr024 | 17.6 |
| fr027 | 14.1 |
| fr029 | 17.8 |
| candytron | 15.3 |
| brullwurfel | 23.3 |
| farbomat | 27.3 |
| kkrieger | 8.9 |
| pzero_new | 4.9 |
| v2_zeitmaschine_new | 14.0 |
| debris_ost | 11.8 |
| fr-052_platinum | 14.3 |
| stars | 36.6 |
