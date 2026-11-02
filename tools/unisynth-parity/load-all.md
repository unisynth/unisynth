# Load-all check (2026-11-02)

Change unify-v2-algorithms 6.9 (spec "No song left behind"): `bun tools/unisynth-parity/load-all.ts --externals`.
Every song is validated, exported and loaded into the song player (`song check`); JSON files must declare `song/2`
(patches `us-patch/2`, the library `us-library/2`). Sources are converted by this tree's `synth-convert` first.

| Location | Kind | Recorded | Found | Loaded | Status |
|---|---|---|---|---|---|
| project/music/*.song.json | tracked | 8 | 8 | 8 | ok |
| project/music/patches/*.patch.json | tracked | 2 | 2 | 2 | ok |
| project/music/released/*.song.json | tracked | 28 | 28 | 28 | ok |
| project/music/released/v2/*.song.json | tracked | 347 | 347 | 347 | ok |
| DAW instrument templates (ui/app/tracks.ts, main.ts) | tracked | 2 | 2 | 2 | ok; DAW test and bench fixtures are checked by the JS tests |
| research/us-size-probe/out/songs/*.song | ignored | 27 | 27 | 27 | ok; re-converted by measure.sh |
| research/us-size-probe/out/songs/legacy/*.song | ignored | 16 | 16 | 16 | ok; WaveSabre sources: converted, then loaded |
| project/music/.library.json | ignored | 1007 | 1007 | 1007 | ok; one song per entry (instrument and its effects) |
| research/tbl-mini-cauldron/out | ignored | 3 | – | – | absent |
| synth-lab/ws/*.bin + refs/*.bin (→ ws/unified) | external | 56 | 56 | 56 | ok; ws/unified (52 old-layout conversions, read by nothing) rewritten from refs/ |
| synth-lab/tunefish/media/tf4modules/*.tfm | external | 2 | 2 | 2 | ok |
| synth-lab/v2songs/acceptance/**/*.v2m | external | 15 | 15 | 15 | ok |
| synth-lab/v2songs/fr_public/**/*.v2m | external | 20 | 20 | 20 | ok |
| synth-lab/v2songs/modland/**/*.v2m | external | 325 | 325 | 324 (+1 unconvertible source) | ok |
| synth-lab/v2songs/carved/**/*.v2m | external | 1 | 1 | 1 | ok |
| synth-lab/v2songs/parity/**/*.song | external | 18 | 18 | 18 | ok; conversions the V2 parity run rewrites each run (the 39 stale ones of an older layout, `parity/aux/`, moved to `unify/stale-parity-aux`) |

No failures.

## Sources that never converted (reported, `UNCONVERTIBLE`)

- `v2songs/modland/KB--fr-013_flybye-loader.v2m`: the file is truncated (patches end past the data)
