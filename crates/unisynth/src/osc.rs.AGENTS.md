# src/osc.rs (sidecar: full detail of the AGENTS.md row)

Pull-only (not auto-injected).

- Types: `Kind` (`Off`, `Pm`, `Blit`, `Sampler`, `Wt`, `V2` (naive generators), `Clocked`, `Pts` and `Pad` (add-tf3-import: `us-osc-points`/`us-osc-pad`; a gap type id → `BadChunk`)); `Kind::from_type` →
  `UnsupportedModule` for a type whose module is off (`OSC_PM/BLIT/WAVETABLE/SAMPLER/NAIVE/CLOCKED`), `Kind::all(values)`.
  Per-voice `Slot {kind, phase, prev, integral, smp, fresh, v2s, clk}` (`new(kind, phase)`, `start`, `clocked_start`), control
  values `SlotT` (inc, gain, mix, pan l/r, pm_out, feedback, wf, pw, leak, det, spread, drive, layers, smp cfg, `v2`, `clk`,
  `combine`), `SlotRamps` (`pan()`, `set`/`to`/`retarget`), `sample(slot, wt, smp, ramps, pm_in)` → `(left, right, send)`
  (unpanned output, send for the next slot's phase); `sample` also takes the slot's `Pts` and PAD table (add-tf3-import: `Pts`/`Pad` arms apply the gain before the decimation stage).
- Phase-mod (Falcon): `sin + wf·(sin 3φ/3 + sin 5φ/5)`, input `(fb·prev + pm_in)/2π` turns, send = output × `pm_out`.
  `Slot::start` restarts PM at `start_phase(draw, p, inc)`; saw/pulse scales its build-time draw at its first note only
  (`fresh`) and then runs on.
- Blit (Slaughter): integrated BLIT in turns: sinc impulse (+ one `pw` later for the pulse), leaky integral
  (`leak = 0.998^(44100/rate)`), DC term `−0.996·inc`, output negated like Slaughter's `−16` gain; start phases drawn once per
  voice at build from the instrument rng.
- Wt (Tunefish): per layer `|inc ± det ± k·spread|` on the left/right phase, non-interpolated table read, drive then hard
  clip, layers summed; output `(L·pl, R·pr)` × gain × mix, send = mean × `pm_out`. Sampler: `inc` = step in sample units per
  frame; output `y·gain·mix·pan`, send = `y·pm_out`. Pm/Sampler/Blit arms guarded by `cfg!(feature)`.
- Naive generators (`Kind::V2`, osc types `NAIVE_SAW..=AUX_B`, `naive_mode(t)` → `v2mode` 1..7): `V2Slot` (cnt, nf, nseed,
  init) and `V2T` (`mode`, `brpt`, `nffrq`, `nfres`, quirks `native_sin`, `integer_fm`, `old_noise`) in `v2::Opt`; `v2_sample` →
  `f32` before pan. `v2_aux(p, i)`: aux A/B oscillator, needs `us-strip`; `v2_sample(…, aux: [(l, r); 2])` reads the
  selected bus.
- Combine: `combine::{ADD,MUL,SET}`, `mix_in(sum, mode, a, b, pl, pr)` (one combine for every slot, mono sum unpanned),
  `COMBINE` = `us-osc-combine`; test `ring_on_a_band_limited_slot`.
- Clocked (osc type 6, `us-osc-clocked`): runs at its phrase clock (`ClockedT.clock` = phrase frames per output frame):
  `clocked_frame(slot, pitch, &ClockedT)` = one original frame (16-bit-phase generator + `clocked_filter`), `clocked()`
  resamples with a 4-point Catmull-Rom over `ClockedSlot.h` (latency 2 phrase frames), then `ClockedLp` (4th-order Butterworth
  at `0.45·clock`, bypass at clock ≥ 1). Waves: saw (+ saw ×1.02 pair), square, spike (`spike7` u⁷, last-voice quirk, v1
  sign/negation, v2 16-bit `t·V`), pulse, noise, fold; v2 clamp ±32767, output /32768. Version `choice(p, 3)` (0 v1, ½ v2 in a
  glide, 1 v2) → `ClockedT.glide`; a glide holds a word above 32767 as `w − 65536`.
- `ClockedT::from_shape(sh, rate, mul_p, phase_p, word_p)`: clamps; `mul_p` = `o::FEEDBACK`·4 compounds base/amount k times
  (f32 per product) from the retrigger byte `2k + accent` (`shape::CLK_RETRIG`), then ×4 (v1) / ×2 (v2) on accent;
  `phase2` from `o::PM_OUT` byte (0 keep, b ≥ 1 → (b − 1)/255 turn); `word` from `o::FIXED_HZ` (b ≥ 1 → `rint(pitch) + b − 128`);
  `clocked_age` (`o::TRACKING` byte → `age`, (b − 1)·3530/8 phrase frames), `clocked_t0(velocity)` = (velocity − 1)·3530/8.
- Clocked filter: `ClockedF {a, b, d, q1}` in `ClockedT.flt` (None when base and amount are 0); `clocked_filter` (original
  recurrence, DC gain −1, saturates `y`/`prev` at ±`CLK_SATURATE` 65536, caps `v1`; NaN → −saturate); `clocked_start(t0, &ClockedT)`
  = retrigger (`prev` 0, `v1 = a·d^age` or `a`, `y` runs on, second saw runs on unless `phase2` set); `ClockedSlot::flt`
  latches the settings at a non-legato note-on. The clocked arm sends 0 into the next slot.
- Tests: `ring_on_a_band_limited_slot`, `an_unstable_rising_filter_saturates_instead_of_running_off`, wavetable unit tests,
  `kinds_follow_the_type_parameter_and_the_build`, `mod clocked` against `osc/cauldron_vectors.rs` (bit-exact, spike ≤ 3 LSB)
  and `cauldron_filter_vectors.rs` (within 1e-4 of peak), phrase-clock resampling, shape decode, word/age/retrigger/phase bytes,
  `the_filter_keeps_the_settings_of_its_note_on`, reconstruction low-pass.
- See change: shrink-unified-synth, add-v2-import (5.2), add-cauldron-xm-import, add-cauldron-latched-lanes,
  unify-v2-algorithms 5.1, 6.5–6.8.

## Former AGENTS.md row (moved, full text)

- Oscillator slots: `Kind` (`Pm`, `Blit`, `Sampler`, `Wt`, `V2` naive, `Clocked`;
- `from_type` → `UnsupportedModule`), `Slot`, `SlotT`/`SlotRamps`, `sample` → `(left, right, send)`, `mix_in` + `combine::*`, `v2_sample`, clocked generator/filter/resampler.
- `Kind::Pair` (`us-osc-pair`, type position 5): `PairT` (wave, shift, ratios `2 − d`/`d`, sign; zero-size stand-in without the module), `pair_wave(t, wave)` (`x = 2(t − round t)`: sine `sin πx`, square, saw), `Slot.phase_b` (second copy's f64 phase), `Slot::start` resets both. See change: add-elevated-import.
