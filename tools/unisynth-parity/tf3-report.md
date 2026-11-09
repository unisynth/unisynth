# Tunefish 3 parity (change add-tf3-import)

Generated 2026-11-09 by `bun tools/unisynth-parity/tf3.ts`. Reference: the original Tunefish 3 player built from the Enigma-Studio-3 checkout in its player configuration (`tools/tf3-parity/build-ref.sh`, player mode); ours: `synth-convert e3prj` (player mode) → `unisynth` (`--profile fast`). Metrics: loudness within 1 dB, octave bands 63 Hz–16 kHz within 3 dB (bands 24 dB under the loudest skipped), onset-strength alignment (lag within 384 frames, correlation ≥ 0.5); renders cut to the shorter one. Projects, renders and captures stay outside the repository.

| Item | Result | Loudness Δ dB | Worst judged band Δ dB | Alignment lag / corr | Seconds | Warnings |
|---|---|---:|---:|---|---:|---|
| points-step | pass | -0.11 | -0.13 (8000 Hz) | 0 / 1.00 | 4.31 |  |
| points-linear | pass | -0.23 | -1.04 (1000 Hz) | 0 / 0.99 | 4.31 |  |
| points-spline | pass | -0.28 | -0.28 (500 Hz) | 0 / 0.99 | 4.31 |  |
| points-4-mixed | pass | -0.29 | -0.31 (500 Hz) | 0 / 1.00 | 4.31 |  |
| unison-spread | pass | -0.40 | -2.31 (250 Hz) | 0 / 0.99 | 12.99 |  |
| sub-oscillator | pass | 0.55 | -2.54 (1000 Hz) | 0 / 0.98 | 12.99 |  |
| high-notes | pass | -0.26 | -0.27 (1000 Hz) | 0 / 0.99 | 4.31 |  |
| drive | pass | -0.35 | -0.36 (250 Hz) | 0 / 1.00 | 4.31 |  |
| octave-down | pass | -0.27 | -0.28 (125 Hz) | 0 / 0.98 | 4.31 |  |
| pad-single | pass (accepted) | -0.29 | -1.18 (250 Hz) | 0 / 0.88 | 26.59 |  |
| pad-detune | pass (accepted) | 0.76 | 4.38 (125 Hz) | 0 / 0.88 | 26.59 |  |
| pad-spread | pass (accepted) | -1.46 | -1.51 (1000 Hz) | 0 / 0.86 | 26.59 |  |
| pad-gauss | pass (accepted) | 1.24 | 2.47 (125 Hz) | 0 / 0.87 | 26.59 |  |
| points-and-pad | pass (accepted) | -0.04 | 3.40 (63 Hz) | 0 / 0.90 | 26.59 |  |
| noise-band | pass | -0.06 | -0.50 (500 Hz) | 0 / 1.00 | 4.31 |  |
| lowpass-env | pass | -0.28 | -0.28 (500 Hz) | 0 / 0.98 | 4.31 |  |
| highpass | pass | 0.17 | 2.79 (1000 Hz) | 0 / 0.84 | 4.31 |  |
| gate | pass | -0.36 | -0.41 (125 Hz) | 0 / 0.94 | 4.31 |  |
| lfo-sine-cutoff | pass | -0.31 | -0.39 (500 Hz) | 0 / 0.98 | 4.31 |  |
| lfo-ramp-volume | pass | -0.09 | -0.10 (500 Hz) | 0 / 0.99 | 4.31 |  |
| lfo-square-pan | pass | 0.47 | 0.52 (500 Hz) | 0 / 0.99 | 4.31 |  |
| matrix-points | pass | -0.85 | -2.74 (500 Hz) | 0 / 0.99 | 4.31 |  |
| fixed-kick | pass | -0.66 | -1.56 (125 Hz) | 0 / 0.90 | 4.31 |  |
| glide-mono | pass | -0.29 | -0.82 (500 Hz) | 0 / 0.98 | 4.31 |  |
| fx-distortion | pass | -0.04 | 0.07 (2000 Hz) | 0 / 1.00 | 4.31 |  |
| fx-delay | pass | -0.25 | -0.26 (500 Hz) | 0 / 0.98 | 4.31 |  |
| fx-reverb | pass | -0.15 | -0.64 (250 Hz) | 0 / 0.98 | 4.31 |  |
| fx-formant | pass | -0.07 | -0.41 (1000 Hz) | 0 / 0.98 | 4.31 |  |
| velocity | pass | -0.09 | -0.14 (250 Hz) | 0 / 0.99 | 4.31 |  |
| transplant | pass | 0.06 | -0.93 (500 Hz) | 0 / 0.81 | 226.52 | instrument 5 "MAD_Noise": LFO 1 depth 0.84 reaches below 0 in Tunefish 3; clamped; instrument 10 "MAD_Bass": LFO 2 depth 0.91 reaches below 0 in Tunefish 3; clamped; instrument 10 "MAD_Bass": OscSlop 0.86 (up to ±5.1 Hz of random detune per note) is not converted; instrument 10 "MAD_Bass": effect slot 2: EQ is not in the Tunefish 3 player; dropped |

30 of 30 items pass.

Accepted wider tolerances: pad-single (loudness ±2 dB, bands ±4.5 dB: random PAD and note phases: the original differs from itself by up to 1.9 dB loudness and 3.7 dB per band); pad-detune (loudness ±2 dB, bands ±4.5 dB: random PAD and note phases: the original differs from itself by up to 1.9 dB loudness and 3.7 dB per band); pad-spread (loudness ±2 dB, bands ±4.5 dB: random PAD and note phases: the original differs from itself by up to 1.9 dB loudness and 3.7 dB per band); pad-gauss (loudness ±2 dB, bands ±4.5 dB: random PAD and note phases: the original differs from itself by up to 1.9 dB loudness and 3.7 dB per band); points-and-pad (loudness ±2 dB, bands ±4.5 dB: random PAD and note phases: the original differs from itself by up to 1.9 dB loudness and 3.7 dB per band).

## Converted Transplant against the capture of the released intro

```text
transplant vs capture: PASS
  offset 1.401 s, gain 0.34 dB (normalised, not judged)
  window lag/corr: 0s:768/0.58 30s:832/0.83 60s:832/0.83 90s:832/0.78 120s:832/0.79 150s:832/0.76 180s:832/0.75
  octaves 63:-0.86 125:0.37 250:0.27 500:-1.09 1000:-0.30 2000:-0.45 4000:-0.49 8000:0.33
  onset timing agrees in 7/7 windows ok
```

## Converted Pandora (2011 engine, `--engine 2011`) against the capture of the released intro

No reference renderer exists for the 2011 engine; the capture is the only reference (change add-tf3-pandora).

```text
pandora vs capture: PASS
  offset 7.486 s, gain 9.89 dB (normalised, not judged)
  window lag/corr: 0s:128/0.69 30s:128/0.84 60s:128/0.76 90s:192/0.70 120s:192/0.70 150s:192/0.70 180s:256/0.77 210s:192/0.66
  octaves 63:0.12 125:0.10 250:0.58 500:-1.74 1000:-2.49 2000:-2.14 4000:-2.72 8000:3.89
  onset timing agrees in 8/8 windows ok
```

Conversion warnings (instruments): 2011 engine: oscillator offset ±x before the drive, clipped at +1 only, is not played (16); 2011 engine: the point curve is evaluated per sample with its mean kept; played from the 257-sample table (16); 2011 engine: no decimation stage; the unified point slot runs one (16); 2011 engine: each unison layer and sub pass is clipped on its own; the unified synth clips their sum (9).

Known differences of the 2011 engine and their measured effect: `research/tunefish3-functional-spec.md` §10.5.

## Known differences

- The point cycle is per voice (Tunefish 3 shares one per instrument: a voice hears the cycle of the voice before it when their modulation differs). Measured effect on Transplant: none (no instrument modulates its points).
- Drive, clip and the decimation stage run per slot (Tunefish 3 drives each sum, then filters the oscillator and PAD sum; its ladder saturates `y − y³/6`). Measured: a sub-oscillator at drive 0.6 differs by 0.59 dB loudness and at most 2.4 dB per band over 32 notes, as without drive (0.55 dB, 2.5 dB).
- MOD1–4 (editor only) become per-instrument controllers; Tunefish 3 sets them per voice. Measured effect on Transplant: none (no effect words; player mode drops them).
- LFO depths whose Tunefish 3 value goes below 0 (sine past depth 0.82, ramp down past 0.71) are clamped (a warning names them).
- Slop, routes to detune or spread, and a PAD voice without a volume envelope (it drones in Tunefish 3) are not reproduced (warnings name them).
- PAD phases are random in both engines: the table's magnitude spectrum matches, the waveform does not.
