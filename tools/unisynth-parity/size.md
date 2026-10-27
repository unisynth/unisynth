i686 `no_std` probe, opt-level z, LTO, static base (`research/us-size-probe`, `measure.sh`), the song built in as Rust (`synth-convert rust`,
`us-trusted`). Raw and packed bytes include the song's own bytes (the "song" column); the build lists exactly the features
`synth-convert features` names for the song.

| build | song B | raw B | xz -9e B | squishy 0.1.3 B | features |
|---|---:|---:|---:|---:|---|
| all-modules | 3401 | 92160 | 44152 | 34304 | us-all |
| global-groove | 559 | 29696 | 15364 | 13824 | us-noise,us-flt-svf,us-env-timed |
| elysian | 63301 | 121344 | 52684 | 45056 | us-osc-pm,us-osc-blit,us-osc-sample,us-sample-gsm,us-noise,us-flt-svf,us-env-timed,us-fx-distortion-shaper,us-fx-crusher,us-fx-eq-parametric,us-fx-delay-tempo,us-fx-reverb-rooms,us-fx-compressor,us-voice-lfo,us-voice-unison,us-voice-oneshot,us-matrix,us-automation |
| engage | 80254 | 150016 | 46972 | 37888 | us-osc-pm,us-osc-blit,us-osc-sample,us-sample-gsm,us-sample-dls,us-noise,us-flt-svf,us-env-timed,us-fx-distortion-shaper,us-fx-crusher,us-fx-eq-parametric,us-fx-delay-tempo,us-fx-flanger-phaser,us-fx-reverb-freeverb,us-fx-reverb-rooms,us-fx-compressor,us-voice-lfo,us-voice-unison,us-voice-oneshot,us-matrix,us-automation |
| eladamri | 32247 | 91136 | 37828 | 30720 | us-osc-pm,us-osc-blit,us-osc-sample,us-sample-gsm,us-noise,us-flt-svf,us-env-timed,us-fx-distortion-shaper,us-fx-eq-parametric,us-fx-delay-tempo,us-fx-reverb-rooms,us-fx-compressor,us-voice-lfo,us-voice-glide,us-voice-mono,us-voice-unison,us-matrix,us-automation |
| no-remorse | 57534 | 103424 | 27152 | 22528 | us-osc-wt,us-noise,us-flt-ladder,us-flt-rbj,us-env-timed,us-env-rate,us-fx-distortion-power,us-fx-eq-3band,us-fx-delay-ms,us-fx-reverb-compact,us-voice-lfo,us-matrix |
| tf4testsong | 12279 | 57856 | 25304 | 20992 | us-osc-wt,us-noise,us-flt-ladder,us-flt-rbj,us-env-timed,us-env-rate,us-fx-distortion-power,us-fx-eq-3band,us-fx-delay-ms,us-fx-reverb-compact,us-voice-lfo,us-matrix |
| turtles | 97922 | 142848 | 31164 | 25088 | us-osc-wt,us-noise,us-flt-ladder,us-flt-rbj,us-env-timed,us-env-rate,us-fx-distortion-power,us-fx-delay-ms,us-fx-reverb-compact,us-voice-lfo,us-voice-glide,us-matrix |

Legacy figures (`research/softsynth-research.md`): WaveSabre port, all 13 devices + 1 KB test sample: 71,168 raw / 34,472 xz / 27,648 squishy;
Slaughter only: 28,672 / 15,968 / 14,336. Tunefish-compatible port: 41,472 raw with every effect, 30,208 without.
