# Unified synth parity report

Metrics only, no audio. Regenerate with `bun tools/unisynth-parity/parity.ts` (design D11).

Generated 2026-10-26. Tolerances: loudness ±1 dB (BS.1770-4), ±3 dB per octave band 63 Hz–16 kHz (bands more than 24 dB below the loudest reference band are not compared), onsets within 100 frames (WaveSabre) or 512 frames (Tunefish), timing by the cross-correlation of the onset-strength curves: lag within that window and correlation ≥ 0.5. (Peak-picked onsets are listed too, but not judged: for two different engines the picker keeps a different one of two neighbouring hits in each render.)

## Retirement: READY

Every device and reference song passes and the A/B listening is signed off.

## Device level

Every WaveSabre test song (`crates/wavesabre/tests/songs.rs`) and every Tunefish synthetic patch (`crates/tfsynth/tests/patches.rs`): 43 of 43 pass.

| item | engine | loudness Δ dB | worst band Δ dB | onset lag (frames) | onset corr | peak onsets matched | result |
|---|---|---:|---:|---:|---:|---:|---|
| adultery | ws | -0.00 | -0.16 | 0 | 1.00 | 3/3 (+0) | pass |
| adultery_pingpong_reverse | ws | -0.00 | -0.01 | 0 | 1.00 | 1/1 (+0) | pass |
| cathedral | ws | 0.11 | 0.89 | 0 | 1.00 | 1/1 (+0) | pass |
| chamber | ws | 0.11 | 0.89 | 0 | 1.00 | 1/1 (+0) | pass |
| crusher | ws | 0.09 | 0.85 | 0 | 1.00 | 1/1 (+0) | pass |
| echo | ws | 0.05 | 0.89 | 0 | 1.00 | 1/1 (+0) | pass |
| falcon | ws | 0.00 | -0.00 | 0 | 1.00 | 2/2 (+0) | pass |
| leveller | ws | 0.01 | -1.90 | 0 | 0.95 | 1/1 (+1) | pass |
| scissor | ws | -0.02 | -1.07 | 0 | 1.00 | 1/1 (+0) | pass |
| slaughter_automation | ws | 0.11 | 0.89 | 0 | 1.00 | 1/1 (+0) | pass |
| slaughter_mono_legato | ws | -0.06 | 0.76 | 0 | 1.00 | 1/1 (+0) | pass |
| smasher_sidechain | ws | -0.02 | 0.58 | 0 | 1.00 | 1/1 (+0) | pass |
| specimen_loop | ws | -0.00 | 0.02 | 0 | 1.00 | 1/1 (+0) | pass |
| thunder | ws | -0.01 | -0.03 | 0 | 0.98 | 8/8 (+0) | pass |
| twister | ws | 0.04 | 0.89 | 0 | 1.00 | 1/1 (+0) | pass |
| twister_phaser | ws | 0.05 | 0.89 | 0 | 1.00 | 1/1 (+0) | pass |
| env_adsr_routes | tf | -0.17 | -9.85 | 0 | 0.68 | 6/6 (+0) | pass |
| filter_bp_notch | tf | 0.08 | 4.23 | 0 | 1.00 | 6/6 (+0) | pass |
| filter_lp_hp | tf | 0.12 | 48.28 | 0 | 0.67 | 6/6 (+25) | pass |
| fx_chorus | tf | -0.38 | -1.53 | 0 | 0.91 | 6/6 (+0) | pass |
| fx_delay | tf | -0.07 | -0.97 | 0 | 1.00 | 1/1 (+0) | pass |
| fx_distortion | tf | -0.01 | -0.95 | 0 | 1.00 | 6/6 (+0) | pass |
| fx_eq | tf | 0.00 | -1.33 | 0 | 0.97 | 6/6 (+0) | pass |
| fx_flanger | tf | -0.19 | -0.66 | 0 | 0.96 | 6/6 (+0) | pass |
| fx_formant | tf | -0.12 | -0.51 | 0 | 0.97 | 6/6 (+0) | pass |
| fx_reverb | tf | -0.14 | -1.00 | 0 | 1.00 | 1/1 (+0) | pass |
| gen_drive_octave_slop | tf | 0.14 | 8.40 | 0 | 0.94 | 6/6 (+0) | pass |
| gen_fixed_freq | tf | -0.03 | 24.05 | 0 | 0.79 | 6/6 (+0) | pass |
| gen_mono_glide | tf | 0.17 | 3.79 | 0 | 0.81 | 6/6 (+0) | pass |
| gen_spectrum | tf | 0.18 | 13.50 | 0 | 0.82 | 6/6 (+0) | pass |
| gen_unison_inharmonic | tf | 0.13 | 10.88 | 0 | 0.85 | 6/6 (+0) | pass |
| lfo_ramp_down | tf | 0.07 | 14.17 | 0 | 0.78 | 6/6 (+0) | pass |
| lfo_ramp_up | tf | -0.18 | 14.90 | 0 | 0.78 | 6/6 (+0) | pass |
| lfo_random | tf | 0.05 | 22.47 | 0 | 0.77 | 6/6 (+0) | pass |
| lfo_sine | tf | -0.20 | 15.00 | 0 | 0.78 | 6/6 (+0) | pass |
| lfo_square | tf | -0.11 | 12.16 | 0 | 0.78 | 6/6 (+0) | pass |
| mod_wheel_self_mod | tf | 0.01 | -0.89 | 0 | 0.97 | 6/6 (+0) | pass |
| noise_bandpass | tf | 0.01 | -2.00 | 0 | 1.00 | 6/6 (+0) | pass |
| unison_0.2_spread_0.4_detune_0 | tf | 0.20 | 14.46 | 0 | 0.89 | 6/6 (+0) | pass (seed mean) |
| unison_0.5_spread_0.4_detune_0 | tf | 0.10 | 14.62 | 0 | 0.88 | 6/6 (+0) | pass (seed mean) |
| unison_0.5_spread_0_detune_0.4 | tf | 0.06 | 15.69 | 0 | 0.89 | 6/6 (+0) | pass (seed mean) |
| unison_1_spread_0.4_detune_0.4 | tf | -0.01 | 16.41 | 0 | 0.89 | 6/6 (+0) | pass (seed mean) |
| unison_1_spread_0_detune_0 | tf | -0.64 | 15.39 | 0 | 0.88 | 6/6 (+0) | pass (seed mean) |

## Reference songs

6 of 6 pass.

| item | engine | loudness Δ dB | worst band Δ dB | onset lag (frames) | onset corr | peak onsets matched | result |
|---|---|---:|---:|---:|---:|---:|---|
| global-groove | ws | 2.08 | 3.59 | 0 | 1.00 | 32/32 (+0) | pass (seed mean) |
| elysian | ws | 0.06 | 0.26 | 0 | 0.97 | 138/153 (+11) | pass |
| engage | ws | -0.02 | 0.27 | 0 | 0.95 | 66/80 (+8) | pass |
| eladamri | ws | -0.02 | 1.93 | 0 | 0.98 | 255/262 (+36) | pass |
| no-remorse | tf | -0.21 | -1.74 | 64 | 0.92 | 217/352 (+3) | pass |
| tf4testsong | tf | 0.04 | -1.30 | 0 | 0.84 | 1/1 (+0) | pass |

## Judged on several random seeds

The reference is one random draw, so these were rendered with 8 seeds; the mean of ours must agree within the usual tolerance plus twice the spread between seeds (loudness 1 dB, bands 3 dB):

- `unison_0.2_spread_0.4_detune_0` (unison layers start at random phases): loudness sd 0.21 dB, band sd 1.10 / 0.58 / 1.87 / 0.63 / 0.47 / 0.49 / 0.18 / 0.13 / 0.09 dB (63 Hz … 16 kHz)
- `unison_0.5_spread_0.4_detune_0` (unison layers start at random phases): loudness sd 0.24 dB, band sd 1.16 / 0.69 / 0.89 / 0.59 / 0.33 / 0.23 / 0.17 / 0.08 / 0.07 dB (63 Hz … 16 kHz)
- `unison_0.5_spread_0_detune_0.4` (unison layers start at random phases): loudness sd 0.40 dB, band sd 1.86 / 0.72 / 1.26 / 0.92 / 0.42 / 0.61 / 0.35 / 0.26 / 0.24 dB (63 Hz … 16 kHz)
- `unison_1_spread_0.4_detune_0.4` (unison layers start at random phases): loudness sd 0.32 dB, band sd 1.78 / 1.20 / 0.62 / 0.45 / 0.40 / 0.22 / 0.13 / 0.03 / 0.03 dB (63 Hz … 16 kHz)
- `unison_1_spread_0_detune_0` (unison layers start at random phases): loudness sd 0.80 dB, band sd 2.04 / 1.08 / 2.17 / 1.67 / 1.24 / 0.68 / 0.42 / 0.31 / 0.55 dB (63 Hz … 16 kHz)
- `global-groove` (every sound is a 2 ms noise burst): loudness sd 0.41 dB, band sd 5.10 / 4.74 / 3.89 / 3.05 / 2.80 / 1.89 / 1.16 / 0.65 / 0.80 dB (63 Hz … 16 kHz)

## Accepted differences

These are judged against a wider tolerance on purpose, because the unified synth does not run a filter that is set open:

- `global-groove`: loudness ±2.5 dB, bands ±4 dB (noise through a low-pass at the top of its range, which is passed through)

## Converter warnings

- `gen_drive_octave_slop`: device 0 (Tunefish): GenSlop 0.60 (up to ±1.7 Hz of random detune per voice) is not converted

## Speed (one thread)

Rendering the song to 16-bit PCM on this machine, legacy engine and converted song alike, with the speed build (`[profile.fast]`: the release profile at opt-level 3). The floor is the spec's bar for the unified synth: Elysian ≥ 5×, no_remorse ≥ 10× real time.

| song | audio s | legacy render s | legacy × real time | unified render s | unified × real time | floor |
|---|---:|---:|---:|---:|---:|---|
| global-groove | 8.1 | 0.010 | 837.6 | 0.012 | 655.2 |  |
| elysian | 217.8 | 30.621 | 7.1 | 22.719 | 9.6 | ≥ 5× ok |
| engage | 217.9 | 36.158 | 6.0 | 32.732 | 6.7 |  |
| eladamri | 191.2 | 14.854 | 12.9 | 8.374 | 22.8 |  |
| no-remorse | 205.6 | 13.951 | 14.7 | 19.582 | 10.5 | ≥ 10× ok |
| tf4testsong | 62.0 | 1.721 | 36.1 | 2.374 | 26.1 |  |

On the profile the player ships with (`release`, opt-level z, built for size) the same songs run slower, the unified synth more so than the legacy engines (its code is more generic and leans on the optimiser to inline it):

| song | legacy × real time | unified × real time |
|---|---:|---:|
| elysian | 5.2 | 6.5 |
| no-remorse | 14.3 | 8.0 |

## Size

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

## A/B listening

Decision: **approved** by robson on 2026-10-26.

A/B listening of the six reference songs (renders/wavesabre, renders/tfsynth, renders/unisynth): "seems ok". The WaveSabre songs and Global Groove he heard were rendered at 12:50 that day, before the libm replacement and the open-filter bypass (Global Groove and Eladamri are slightly brighter since, see Accepted differences); the Tunefish songs were re-rendered at 17:16 and include the libm replacement. A listen to the current build is recommended before retire-legacy-synths is archived.
