# src/instrument/tests/golden.rs (sidecar: full detail of the AGENTS.md row)

Pull-only (not auto-injected).

- Self-goldens: FNV-1a 64 of the f32 stereo output of the 35 synthetic patches in `tests/golden.txt` (envelope laws,
  routed-only env, LFO, matrix, unison, mono/glide, steal, PM/FM, fixed Hz, BLIT, wavetable, clocked (`clocked_v1_saw_pair`,
  `clocked_v2_spike_clocked`, `clocked_v1_filtered_resonant`, stored with `osc_type::encode`), DLS/GSM sampler, noise, each
  filter type, series filters, ladder at full resonance, cutoff/resonance routes) playing `phrase()` at 44.1 kHz.
  `UPDATE_GOLDEN=1 cargo test -p unisynth golden` rewrites the file (review the diff).
- Every patch renders identically at call sizes 512 and 77. The line count of `golden.txt` must equal the patch count.
- `loadable()`: a reduced build plays every golden patch it can load and must match the full-build hash (more than half
  must load). With feature `probe` the goldens play with taps on (must match). Needs all oscillator, noise, filter features.
- See change: shrink-unified-synth, add-cauldron-xm-import.

## Former AGENTS.md row (moved, full text)

- Self-goldens: FNV-1a 64 of 35 synthetic patches playing `phrase()` against `tests/golden.txt` (`UPDATE_GOLDEN=1` rewrites); same output at call sizes 512 and 77; reduced builds match via `loadable()`.
