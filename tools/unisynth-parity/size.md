i686 `no_std` probe, opt-level z, LTO, static base (`research/us-size-probe`, `measure.sh`), the song built in as Rust (`synth-convert rust`,
`us-trusted`). Raw and packed bytes include the song's own bytes (the "song" column); the build lists exactly the features
`synth-convert features` names for the song.

| build | song B | raw B | xz -9e B | squishy 0.1.3 B | features |
|---|---:|---:|---:|---:|---|
| all-modules | 3401 | 135680 | 64788 | 49152 | us-all |
| global-groove | 559 | 30720 | 15536 | 14336 | us-noise,us-flt-svf,us-env-seconds |
| elysian | 63301 | 122880 | 53044 | 45056 | us-osc-pm,us-osc-blit,us-osc-sample,us-sample-gsm,us-noise,us-flt-svf,us-env-seconds,us-fx-distortion-shaper,us-fx-crusher,us-fx-eq-parametric,us-fx-delay-tempo,us-fx-reverb-rooms,us-fx-compressor,us-voice-lfo,us-voice-unison,us-voice-oneshot,us-matrix,us-automation |
| engage | 80254 | 151040 | 47328 | 38400 | us-osc-pm,us-osc-blit,us-osc-sample,us-sample-gsm,us-sample-dls,us-noise,us-flt-svf,us-env-seconds,us-fx-distortion-shaper,us-fx-crusher,us-fx-eq-parametric,us-fx-delay-tempo,us-fx-flanger-phaser,us-fx-reverb-freeverb,us-fx-reverb-rooms,us-fx-compressor,us-voice-lfo,us-voice-unison,us-voice-oneshot,us-matrix,us-automation |
| eladamri | 32247 | 92160 | 38332 | 31232 | us-osc-pm,us-osc-blit,us-osc-sample,us-sample-gsm,us-noise,us-flt-svf,us-env-seconds,us-fx-distortion-shaper,us-fx-eq-parametric,us-fx-delay-tempo,us-fx-reverb-rooms,us-fx-compressor,us-voice-lfo,us-voice-glide,us-voice-mono,us-voice-unison,us-matrix,us-automation |
| no-remorse | 57534 | 103936 | 27388 | 22528 | us-osc-wt,us-noise,us-flt-ladder,us-flt-rbj,us-env-seconds,us-env-rate,us-fx-distortion-power,us-fx-eq-3band,us-fx-delay-ms,us-fx-reverb-compact,us-voice-lfo,us-matrix |
| tf4testsong | 12279 | 58880 | 25544 | 21504 | us-osc-wt,us-noise,us-flt-ladder,us-flt-rbj,us-env-seconds,us-env-rate,us-fx-distortion-power,us-fx-eq-3band,us-fx-delay-ms,us-fx-reverb-compact,us-voice-lfo,us-matrix |
| v2-fr08 | 288511 | 354304 | 47416 | 34816 | us-voice-lfo,us-matrix,us-automation,us-matrix-add,us-env-frame,us-osc-naive,us-flt-feedback,us-drive,us-lfo-frame,us-voice-out,us-strip,us-fx-eq-cut,us-fx-delay-modulated,us-fx-reverb-comb,us-fx-compressor-lookahead,us-chunk-wide |
| v2-brullwurfel | 43485 | 109056 | 34816 | 27648 | us-voice-lfo,us-matrix,us-automation,us-matrix-add,us-env-frame,us-osc-naive,us-flt-feedback,us-lfo-frame,us-voice-out,us-strip,us-fx-eq-cut,us-fx-delay-modulated,us-fx-reverb-comb,us-fx-compressor-lookahead,us-chunk-wide |
| v2-candytron | 79630 | 157184 | 45964 | 35328 | us-voice-lfo,us-matrix,us-automation,us-matrix-add,us-speech,us-env-frame,us-osc-naive,us-flt-feedback,us-flt-parallel,us-drive,us-lfo-frame,us-voice-out,us-strip,us-fx-eq-cut,us-fx-delay-modulated,us-fx-reverb-comb,us-fx-compressor-lookahead,us-chunk-wide |
| v2-kkrieger | 37131 | 114688 | 39900 | 31744 | us-voice-lfo,us-matrix,us-automation,us-matrix-add,us-speech,us-env-frame,us-osc-naive,us-flt-feedback,us-flt-parallel,us-drive,us-lfo-frame,us-voice-out,us-strip,us-fx-eq-cut,us-fx-delay-modulated,us-fx-reverb-comb,us-fx-compressor-lookahead,us-chunk-wide |
| turtles | 98154 | 144384 | 35992 | 28160 | us-osc-wt,us-noise,us-flt-ladder,us-flt-rbj,us-env-seconds,us-env-rate,us-fx-distortion-power,us-fx-delay-ms,us-fx-reverb-compact,us-voice-lfo,us-voice-glide,us-matrix |

turtles: without `ES4_SONGS` (`zoom.tf4m`), exported from `project/music/turtles.song.json` (98,154 B). Its bytes differ
from the `.tf4m` conversion of earlier tables (97,922 B: 142848 raw / 31164 xz / 25088 squishy), so compare it only
with the "before" below, built from the same bytes.

V2 rows (`v2-*`, change add-v2-import): converted with `synth-convert v2m`.

Cost of the V2 work for songs without V2 (xz -9e / squishy 0.1.3 bytes; "before" = the build before add-v2-import):

| song | before | V2 first measured | after gating | now |
|---|---|---|---|---|
| global-groove | 15364 / 13824 | 16912 / 14848 | 15996 / 14336 | 15544 / 14336 |
| elysian | 52680 / 45056 | 54436 / 46080 | 53560 / 45568 | 53044 / 45056 |
| engage | 46988 / 37888 | 48576 / 39424 | 47736 / 38912 | 47344 / 38400 |
| eladamri | 37840 / 30720 | 39688 / 32256 | 38740 / 31744 | 38308 / 31232 |
| no-remorse | 27152 / 22528 | 28760 / 23552 | 27848 / 23040 | 27360 / 22528 |
| tf4testsong | 25320 / 20992 | 26904 / 22528 | 26028 / 21504 | 25540 / 21504 |
| turtles (song.json) | 35836 / 27648 | | | 35988 / 28160 |

"After gating": every V2 branch behind its feature's `cfg!`; V2 voice and filter-route state in `v2::Opt`. "Now",
in addition: the V2 slot, envelope and LFO state in `v2::Opt`; the `flt::Kind::V2` variant only with `us-flt-feedback`; a
trusted build without the V2 kinds decodes choices and effect modes with the old law; chunk escapes behind
`us-chunk-wide` (5-byte pairs without it); `run_ext` only with `us-strip`. The rest (+150 to +470 B xz) is the other
voice-struct growth and the instrument's render split.

Aux buses and the Moog fix (task 5.2, commit `4b8d7ce`) vs `49185ef`, same compo probe in clean worktrees, each song
converted by its own tree (raw / xz -9e / squishy 0.1.3 bytes). The aux path is behind `us-strip`, so songs without V2
are byte-identical; every V2 song has `us-strip` and carries the aux-input plumbing even without an aux oscillator.

| song | before | after | Δ xz |
|---|---|---|---:|
| 7 songs without V2 | | identical | 0 |
| v2-candytron | 157184 / 46056 / 35328 | 157184 / 46148 / 35328 | +92 |
| v2-platinum (aux) | 161792 / 39228 / 30720 | 161792 / 39384 / 30720 | +156 |
| v2-brullwurfel | 109056 / 34820 / 27648 | 109056 / 34992 / 27648 | +172 |
| v2-debris (aux) | 508416 / 51076 / 38400 | 508928 / 51272 / 38400 | +196 |
| v2-kkrieger | 114688 / 39908 / 31744 | 114688 / 40112 / 31744 | +204 |
| v2-fr08 | 354304 / 47816 / 34816 | 354304 / 48192 / 34816 | +376 |

V2 oscillator pan in a unisynth voice (mixed patches), after `4b8d7ce`: v2-fr08 354816 raw / 48060 xz (−132),
v2-brullwurfel 109568 / 35084 (+92), global-groove unchanged (15544). V2-voice renders stay byte-identical.

Legacy figures (`research/softsynth-research.md`): WaveSabre port, all 13 devices + 1 KB test sample: 71,168 raw / 34,472 xz / 27,648 squishy;
Slaughter only: 28,672 / 15,968 / 14,336. Tunefish-compatible port: 41,472 raw with every effect, 30,208 without.
