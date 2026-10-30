## 1. Mixer (`synth-core`)

- [x] 1.1 `Device::quiet_after`, `quiet_skip`, `wake`; `Either` forwards. Verify: `quiet_skip_tests` (echo tail kept, unknown memory, input wakes, wake order and frames).
- [x] 1.2 Serial and threaded steps: receives before automations, sleep decision, `wake`, quiet bookkeeping from every device's output and the input. Verify: `cargo test -p synth-core` with and without `threads,pairs`.
- [x] 1.3 `Mixer::set_skip_quiet`, `Player::set_skip_quiet`. Verify: the `render` example's `US_NO_SKIP=1`.

## 2. unisynth

- [x] 2.1 Feature `us-quiet` (in `us-all`); `UDevice::quiet_skip`; instrument and mode answers; EQ 3band and comb `wake`. Verify: `quiet_after_is_the_memory_of_the_mode`, `a_woken_three_band_ramps_as_if_it_had_run`, `a_woken_comb_sweeps_on_as_if_it_had_run`, `an_instrument_is_quiet_only_without_voices_or_queued_notes`.

## 3. Measure and document

- [x] 3.1 The three TBL songs with and without the skip: within one 16-bit step; render 1.6–2.4× faster. Verify: `tools/unisynth-parity/size.md`.
- [x] 3.2 Size: without `us-quiet` +512 raw / +128 xz / 0 squishy (global-groove). Verify: `size.md`.
- [x] 3.3 `docs/synth/synth-core.md` (steps, quiet tracks), `docs/synth/unisynth.md` (`us-quiet`), AGENTS rows. Verify: `kb_search "quiet track skip"`.
