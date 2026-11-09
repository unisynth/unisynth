# src/flt.rs (sidecar: full detail of the AGENTS.md row)

Pull-only (not auto-injected).

- API: `Kind` (`Off`, `SvfLp/Hp/Bp/Notch/All`, `Pole4Lp/Hp`, `RbjHp/Bp`, `Notch`; `check(ty, forms)` → `UnsupportedModule`
  when the module is off, `of` bypasses at run time), `Forms {feedback, held, steps}` (`PLAIN`, `STEPS`, `of` gated by
  `FORMS`, `read` for checks, `plain`), `coefs(kind, hz, res, rate, forms)`, `coefs_steps(kind, cut, res, rate, forms)`,
  `has_steps`, `STEPS_KINDS`, `transparent(kind, hz, res)`, `OPEN_LP_HZ`, `OPEN_HP_HZ`, pub `run`/`NS`/`NC`, `Slot`
  (stereo state `[f32; 9]`, 5 coefficient `Ramp`s: `set`/`to`/`retarget`, `pass(on)`, `process`).
- Forms: `sv_coefs(f, res, SvForm)` (`SvForm::{INPUT,FEEDBACK}`: input/feedback resonance, interpolated/held oversampling;
  laws `sv_f_hz`, `sv_f_steps`) and `p4_coefs(f, res, P4Form)` (`P4Form::{ONE,TWO}`: feedback before/after the clip,
  1×/2×; laws `p4_f_hz`, `p4_f_steps`); one formula exact for the earlier SVF, V2 LRC and V2 Moog; one-step ladder rounds
  `(in+prev)·p`. High-pass 4-pole = input − low-pass. No denormal bias (no song moved without it). Coefs `[f, a, c, α, 0]`
  (SV) / `[g, p, fb, code, 0]` (4-pole). `FORMS` const compiles the form code only with `us-flt-feedback`/`us-drive`/`us-strip`.
- `Kind::check`: plain SV/4-pole → `us-flt-svf`/`us-flt-ladder`, any other form or the 7-bit law, all-pass and 4-pole
  high-pass → `us-flt-feedback`. `coefs`/`run` arms guarded by `cfg!(feature)`.
- Laws: cutoff in Hz clamped to `1..0.49·rate`; SVF corner is 3/4 of the parameter (WaveSabre `1.5·sin`); RBJ log₁₀2
  bandwidth law; ladder/RBJ/notch clamp `p` to 0..0.85; notch has one sample of input latency. State kept across kinds of one
  layout (`family`), cleared across layouts.
- `transparent`: low-pass/ladder at `OPEN_LP_HZ`, high-pass at `OPEN_HP_HZ`, no resonance. A passed-through slot
  (`Slot::pass(on)` before `set`/`to`/`retarget`) is not run or updated; on the way out `waking` primes the state from the
  last frame (low-pass/ladder hold it, others zero) and jumps to the coefficients. Measured loss when open filters are
  run: SVF LP −0.4 dB at 5 kHz, −1.6 at 10 kHz, −4 at 16 kHz (44.1 kHz), ladder 1.15 dB full scale, RBJ HP within 0.1 dB;
  Eladamri +1.9 dB in the 16 kHz octave when bypassed; wake gap < 0.03 vs always running (5× worse cold).
- `set_run_open(bool)` (feature `probe`, thread-local, editor-only): `transparent` → false, open filters run as in the
  legacy engines.
- Tests: SVF vs f64 port, responses ±1 octave at 44.1/48/96 kHz, forms reproduce the earlier filters (old code kept in
  tests), forms combine freely, 4-pole high-pass on the Hz law, ramps, Nyquist clamp, module gating, transparency figures,
  wake gap.
- See change: add-unified-synth (7.7), shrink-unified-synth, add-v2-import, add-device-live-view,
  unify-v2-algorithms 2.1–2.2, 6.4, 6.5–6.8.

## Former AGENTS.md row (moved, full text)

- Filter slots: `Kind` (SVF ×5, 4-pole ×2, RBJ HP/BP, notch), `Forms`, `coefs`/`coefs_steps` (5 coefficients), `Slot` (stereo state, ramped coefficients, `pass`), `transparent`, `run`;
- `check` → `UnsupportedModule` per `us-flt-*`.

## Tunefish 3 form (add-tf3-import 6.4)
- `f::FORM` 1 on the RBJ and notch types (which ignored it) = Tunefish 3's form: `RbjBp` takes the shelf-slope α `sin ω0/2·√((A+1/A)(1/BW−1)+2)`, `A = 1.059253725`, `BW = 1 − clamp(q, 0, 0.95)` (constant skirt b0 = sin/2); `RbjHp` takes resonance up to 0.95 (same law); `NOTCH` → `Kind::Tf3Notch` (RBJ notch `b = 1, −2cos, 1`, same α, plain biquad, no latency; family 4 with `Notch`, so a form lane keeps the state).
- Needs `us-flt-feedback` like every non-plain form (`Kind::check` → `UnsupportedModule(FILTER_FEEDBACK)`); a lane reaching form 1 in a build without it bypasses (the existing lane contract; `needs` reports the module). Chosen over expanding the type choice: filter types are automatable and an expansion would move off-centre values (doubt review, design D7).
- Tests: `tunefish_3_band_pass_…`, `tunefish_3_high_pass_…`, `tunefish_3_notch_…`.
