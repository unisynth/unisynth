## 1. Converter

- [x] 1.1 `compact::layers` (drop same-tick copies and echo taps, equal-power level of absorbed copies). Verify: unit tests "Stereo pair", "Echo tap", "A louder or later note stays".
- [x] 1.2 `compact::pack` (phrase triggers per record across ratios, reach-limited, class = most frequent moved into range) and stored packing per (instrument, sample). Verify: unit tests "Two channels, two ratios", "Out of reach", no overlap within a track.
- [x] 1.3 `Options { layers, split }`, `--keep-layers`, `--split-pitches`, stderr `layers …`. Verify: both options → byte-identical to the pre-change songs (three songs); default converts, loads, fewer tracks.
- [x] 1.4 Parity tool passes both options. Verify: `bun tools/unisynth-parity/cauldron.ts` songs still PASS.

## 2. Measure, listen, document

- [x] 2.1 Three songs, default vs old layout: tracks, song bytes, xz, render time, loudness and bands against the reference → `tools/unisynth-parity/cauldron-compact.md`; WAV/MP3 renders of both in `research/tbl-mini-cauldron/out/compact/`.
- [x] 2.2 Docs: `docs/synth/unisynth.md` (Mini-cauldron XMs), `crates/synth-convert/AGENTS.md` rows.

## 3. Dropped layers as track effects

- [x] 3.1 `Layers::absorbed` (pair, taps: delay, pan, gain); absorbed level: copies at the kept pitch add amplitudes, detuned copies powers. Verify: unit tests "Detuned pair", absorbed record.
- [x] 3.2 `ambience` (majority rule, most common echo, reverb on phrase tracks) and `Ambience::devices` (tempo delay without feedback, freeverb, make-up volume); `delay_time` shared with `cauldron::effects`; `--dry`. Verify: unit tests; `--dry` and the layered layout byte-identical to before; CLI test.
- [x] 3.3 Freeverb `quiet_after` (unisynth). Verify: `a_freeverb_answers_its_memory`; Jizz renders with and without the skip within one 16-bit step.
- [x] 3.4 Measure layered / dry / compact (`cauldron-compact.ts`), docs, AGENTS rows.
