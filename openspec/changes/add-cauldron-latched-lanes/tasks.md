## 1. Engine

- [x] 1.1 `CauldronSlot::flt` latched at `cauldron_start`. Verify: `the_filter_keeps_the_settings_of_its_note_on`; unisynth suites and goldens.

## 2. Converter

- [x] 2.1 `latched_lane` for retrigger, phase, age. Verify: `cauldron::tests::latched`; parity 85 of 88 unchanged.
- [x] 2.2 Demo songs with `--thin 2`; point counts and sizes in `docs/synth/unisynth.md`.
