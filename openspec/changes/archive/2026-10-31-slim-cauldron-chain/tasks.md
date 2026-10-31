## 1. Converter

- [x] 1.1 `Chain::lp_stages`; `compact::slim_lowpass` (one stage, none from `OPEN_HZ` 16 kHz without a slide). Verify: `the_compact_low_pass_is_one_stage_and_none_where_inaudible`; `--exact-chain` byte-identical.
- [x] 1.2 `compact::clips`, `input_peak` (solo render), `idle_clips`, `remove_device`; tracks checked in parallel. Verify: `an_idle_clip_leaves_the_sound_unchanged_and_a_working_one_is_kept` (every Jizz and Stash song0 phrase); `removing_a_device_shifts_the_later_automations`.
- [x] 1.3 `Options::exact_chain`, `--exact-chain`, stderr `clips N idle left out`; parity tool passes it. Verify: three songs with `--keep-layers --split-pitches --exact-chain` byte-identical to before; parity songs PASS.

## 2. Measure and document

- [x] 2.1 Layered / dry / compact table (`cauldron-compact.ts`), device counts per song; docs (`docs/synth/unisynth.md`), AGENTS rows; demo songs rebuilt.
