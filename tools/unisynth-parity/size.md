i686 `no_std` probe, opt-level z, LTO, static base (`research/us-size-probe`, `measure.sh`). Raw and packed bytes include the song's own
bytes (the "song" column) and the pure-Rust `libm`; the build lists exactly the features `synth-convert features` names for the song.

| build | song B | raw B | xz -9e B | squishy 0.1.3 B | features |
|---|---:|---:|---:|---:|---|
| all-modules | 3401 | 93696 | 44552 | 34304 | us-all |
| global-groove | 559 | 37888 | 19684 | 16896 | us-noise,us-flt-svf,us-env-timed |
| elysian | 63301 | 124416 | 54048 | 46080 | us-osc-pm,us-osc-blit,us-osc-sample,us-sample-gsm,us-noise,us-flt-svf,us-env-timed,us-fx-distortion-shaper,us-fx-crusher,us-fx-eq-parametric,us-fx-delay-tempo,us-fx-reverb-rooms,us-fx-compressor |
| engage | 80254 | 153088 | 48216 | 38912 | us-osc-pm,us-osc-blit,us-osc-sample,us-sample-gsm,us-sample-dls,us-noise,us-flt-svf,us-env-timed,us-fx-distortion-shaper,us-fx-crusher,us-fx-eq-parametric,us-fx-delay-tempo,us-fx-flanger-phaser,us-fx-reverb-freeverb,us-fx-reverb-rooms,us-fx-compressor |
| eladamri | 32247 | 92672 | 38712 | 31744 | us-osc-pm,us-osc-blit,us-osc-sample,us-sample-gsm,us-noise,us-flt-svf,us-env-timed,us-fx-distortion-shaper,us-fx-eq-parametric,us-fx-delay-tempo,us-fx-reverb-rooms,us-fx-compressor |
| no-remorse | 57534 | 107520 | 29260 | 24064 | us-osc-wt,us-noise,us-flt-ladder,us-flt-rbj,us-env-timed,us-env-rate,us-fx-distortion-power,us-fx-eq-3band,us-fx-delay-ms,us-fx-reverb-compact |
| tf4testsong | 12279 | 61952 | 27416 | 22528 | us-osc-wt,us-noise,us-flt-ladder,us-flt-rbj,us-env-timed,us-env-rate,us-fx-distortion-power,us-fx-eq-3band,us-fx-delay-ms,us-fx-reverb-compact |

Legacy figures (`research/softsynth-research.md`): WaveSabre port, all 13 devices + 1 KB test sample: 71,168 raw / 34,472 xz / 27,648 squishy;
Slaughter only: 28,672 / 15,968 / 14,336. Tunefish-compatible port: 41,472 raw with every effect, 30,208 without.
