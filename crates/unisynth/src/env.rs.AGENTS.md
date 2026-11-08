# src/env.rs (sidecar: full detail of the AGENTS.md row)

Pull-only (not auto-injected).

- API: `Env` (`new`, `trigger(keep)`, `release`, `advance(dt, &EnvP, decay_mod)`, `value`, `stage`, `finished`), `Stage`,
  `EnvP` (`law`, `curve`, `a d s r`, `curve_slope`, `sustain_slope`, `amp`, `coarse`, `instant`; `DEFAULT`, `read(p, i)`,
  `live()`, `keeps_level()`), `time_law::{SECONDS,RATE,FRAME}`, `curve::{QUADRATIC,POWER,EXPONENTIAL}`, `check(by_rate)`,
  `check_frame`, `check_settings(p, i)`; private `time(ep, stage, decay_mod)` per stage (lazy), `of_mode`.
- One stage machine for every law, advanced by elapsed time at control rate; `advance` stays out of line (packs better).
- Laws: seconds (WaveSabre, `law::env_ms`), rate (Tunefish per-512-block log-rate steps per second; one Euler step per
  segment; a decay that would rise holds at the peak, sustain fixed at note-on), frame (V2 per-frame steps at 344.53/s).
  Attack linear for every law. Curves: quadratic `(1 − t/T)²` closed form from the level at note-off, power (rate module),
  exponential (base 2, frame module). Sustain slope and amplify need `us-env-frame`.
- `EnvP::read` from `e::LAW`, `e::CURVE` (0 = by law), `CURVE_SLOPE`, `SUSTAIN_SLOPE`, `AMPLIFY`; the curve is read only
  with `us-env-rate` or `us-env-frame` (else quadratic). `coarse` = frame law + quirk `coarse_env` (`us-quirk-coarse-env`).
  `instant`: a seconds-law attack of exactly 0 is instant (voice sets it for clocked instruments; else minimum 1 ms).
- Frame law is re-read every segment (`live`) and keeps the level at note-on (`keeps_level`); other laws take fields at
  note-on.
- Load checks: `check(by_rate)` → `UnsupportedModule(ENV_SECONDS/ENV_RATE)`, `check_frame` → `ENV_FRAME`,
  `check_settings` (law, power curve → rate module, exponential/sustain slope/amplify → frame module). `of_mode`: a build
  with one of seconds/rate runs it for an automated mode it lacks.
- Tests: `mod tests` keeps old per-mode envelopes (`OldEnv`) as references: earlier modes as settings (one block, 0.5 dB
  above −60 dB), sustain slope on a seconds law, WS curves, release from the current value, step-size independence, TF
  recurrence, instant attack. `mod gating`: load checks.
- See change: add-unified-synth, shrink-unified-synth, add-v2-import, add-cauldron-xm-import, unify-v2-algorithms 4.1, 6.5–6.8.

## Former AGENTS.md row (moved, full text)

- ADSR envelopes advanced by elapsed seconds at control rate: `Env`, `EnvP` (law seconds/rate/frame, curve quadratic/power/exponential), `Stage`; load checks → `UnsupportedModule(ENV_*)`; laws compile out per `us-env-*`.
