# src/voice.rs (sidecar: full detail of the AGENTS.md row)

Pull-only (not auto-injected).

- API: `Voice<'a>::new(kinds, rng, wt, smp, insts)` (`wt`: pool-backed `Wt` per wavetable slot, `smp`: sample data per
  sampler slot, `insts`: DLS instrument per slot; draws the slots' start phases), `reset(kinds, rng)` (re-draws phases,
  clears tables), `tick(sh, frame)`, `left()` (frames to the end of the current segment), `render`, accessors (`pitch`,
  `env_value`, `lfo_phase`, `mod_scale/offset`, `detune`, `velocity`, `apply_channel`), `snap(sh)` (probe),
  `impl synth_core::voice::Voice<NoteCx>` (`note_on`, `note_off`, `slide`; `MONO`/`UNISON` from `us-voice-mono`/`-unison`).
  `Shared{p, rate}`, `NoteCx{sh, rng, frame, force, seq}` (`force` = all-notes-off, releases one-shot voices; `seq` = newest
  voice for the channel stage), `CONTROL` 32, `FINE` 8.
- Control state: `start` (segment start) and `end` (one segment later) copies of `Ctl` (`Pitch` with `Move::{Snap, Linear,
  Expo, Hz}`, 4 `Env`, 2 `Lfo`, `Matrix`), `Ramp`s between them; `begin_segment`, `edit` (an event splits a segment on its
  exact frame). Fine window: `FINE` = 8 frames for `FINE_SPAN` = 256 frames after note-on/off. A releasing voice below
  `QUIET` (5e-4) for `QUIET_FRAMES` (256) is freed. Voice ends when env 1 finishes; a one-shot voice ends when every sampler
  slot has run out (`samplers_done`).
- Gain = master × mod × velocity sensitivity × env 1 (env 1 alone with `VoiceSet.env_amp`). `slot_targets` (one for every
  kind; 7-bit oscillator law for naive generators from `o::LAW`) builds slot control values through the matrix; `targets`
  builds a `modmx::Tab` once and skips off slots/filters/band. `Targets.flt` carries `(Kind, coefs, transparent)` per slot;
  the ramp setters call `Slot::pass` first. Filter cutoff = `cutoff(p·Scale) + Offset` clamped 0..19 980 Hz (`cut_hz`),
  resonance `p·Scale + Offset` clamped 0..1. `render` sums oscillators + noise, runs the filter slots in series (or parallel
  with `us-flt-parallel`), then scales by the voice gain ramp; slot combine from `o::COMBINE`.
- `VoiceSet` (env_amp, pan_after, dc, steps) from `vs::{AMP_SOURCE, PAN_STAGE, DC_FILTER, LAW}` (`us-voice-out`); keysync
  `Choice(3)` (`vs::KEYSYNC`); quirks `exact_math`, `integer_fm`, `lcg_noise`, `fixed_seed` (V2 noise seeds `V2_SEEDS`).
  `tuning(p)` = transpose + bend (`law::bend_semitones`). `added(mm, p)`: voice Add slots applied to a copy of the
  parameters, read by `Ctl::advance` (LFO rate, envelopes) and `targets` (matrix eval stays on base values); `Sources` gets
  `lfo_add`/`cc` (only with `us-matrix-add`) and `note`.
- Wavetable: `wt_build` rebuilds at note-on and whenever the voice age passes a multiple of 2048 frames (`WT_CHECK`), from
  the control state advanced one Tunefish block (`WT_STEP` 512) ahead (before this, envelope-scaled harmonics sounded dull
  for 2048 frames: no_remorse basses −10…−23 dB above 1 kHz); note-on draws layer phases (right = left + 0.1·rand) and drift
  `mu ∈ [0, 100)`.
- Sampler: slots start at note-on (start value and reverse through the matrix); pitch step `exp2((semis − 60)/12)·ratio·pitch
  scale`, fixed-Hz mode `hz / 261.62556` (`MIDDLE_C`); per-slot `insts`/`adj`, note-on picks regions before the first targets.
- Clocked slots: `ClockedT` from the shape values per segment; note-on sets the phrase frame from velocity (`osc::clocked_t0`)
  and passes the slot's `ClockedT` to `clocked_start`; `from_shape` gets `o::FEEDBACK` (mul), `o::PM_OUT` (phase byte),
  `o::FIXED_HZ` (word byte); `TRACKING` is the age byte, not pitch tracking. Mono slide is linear in frequency (`Move::Hz`,
  `Pitch::slide_hz_to`: halfway = 1.5×; plain slide stays semitone-linear; `pitch_tests`). `EnvP { instant: true }` at note-on.
  Phrase clock × `2^(tuning/12)`; `Fixed::resample` advances envelopes at that rate. `Declick` (only with `us-osc-clocked`):
  carries the step from a sounding cut note's last output frame to the new note's first, faded over `DECLICK_MS` (1 ms); from
  silence (< `SILENT`, −80 dB) nothing; test `a_declick_carries_only_a_sounding_note`. `slots` is `pub(crate)` for tests.
- Feature gates: `Pitch::advance` snaps and slide snaps without `us-voice-glide`; LFO advance/shape/phase skipped without
  `us-voice-lfo`; one-shot checks need `us-voice-oneshot`; V2 state in `v2::Opt` with every branch behind `cfg!` so builds
  without it drop them; `render(l, r, tap?, aux?)`: `tap` only with `probe` (adds amp-scaled mono in/out per active filter
  slot to the ring; one loop-invariant branch, a generic sink / separate tapped loop measured slower), `aux` only with
  `us-strip`.
- See change: add-unified-synth, shrink-unified-synth, add-device-live-view, add-v2-import (5.2), add-cauldron-xm-import,
  unify-v2-algorithms 5.1–5.2, 6.5–6.8.

## Former AGENTS.md row (moved, full text)

- `Voice<'a>` (`synth_core::voice::Voice<NoteCx>`): start/end control state per 32-frame segment, `Ramp`s, `tick`/`left`/`render`; slot targets via mod matrix, noise, 4 filter slots, wavetable rebuild, samplers, `VoiceSet`, clocked slots + `Declick`;
- `probe` taps.
