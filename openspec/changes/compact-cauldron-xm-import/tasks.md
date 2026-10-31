## 1. Converter

- [x] 1.1 `compact::layers` (drop same-tick copies and echo taps, equal-power level of absorbed copies). Verify: unit tests "Stereo pair", "Echo tap", "A louder or later note stays".
- [x] 1.2 `compact::pack` (phrase triggers per record across ratios, reach-limited, class = most frequent moved into range) and stored packing per (instrument, sample). Verify: unit tests "Two channels, two ratios", "Out of reach", no overlap within a track.
- [x] 1.3 `Options { layers, split }`, `--keep-layers`, `--split-pitches`, stderr `layers …`. Verify: both options → byte-identical to the pre-change songs (three songs); default converts, loads, fewer tracks.
- [x] 1.4 Parity tool passes both options. Verify: `bun tools/unisynth-parity/cauldron.ts` songs still PASS.

## 2. Measure, listen, document

- [x] 2.1 Three songs, default vs old layout: tracks, song bytes, xz, render time, loudness and bands against the reference → `tools/unisynth-parity/cauldron-compact.md`; WAV/MP3 renders of both in `research/tbl-mini-cauldron/out/compact/`.
- [x] 2.2 Docs: `docs/synth/unisynth.md` (Mini-cauldron XMs), `crates/synth-convert/AGENTS.md` rows.
