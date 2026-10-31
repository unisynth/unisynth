## 1. Converter

- [x] 1.1 `thin_lane`. Verify: `cauldron::tests::thinning` (tolerance 0 exact, slide within tolerance with fewer points, jumps stay steps, break frames step).
- [x] 1.2 `Options`, `convert_with`, `--thin`; default byte-identical. Verify: `cmp` of song 1 against the previous build.

## 2. Measure and document

- [x] 2.1 Sizes and sound at 1, 2, 4 bytes. Verify: `docs/synth/unisynth.md` (Thinning).
