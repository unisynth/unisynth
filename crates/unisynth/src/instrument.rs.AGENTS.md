# src/instrument.rs (sidecar: full detail of the AGENTS.md row)

Pull-only (not auto-injected).

- API: `UInstrument` (kind 64 `Device`), `MAX_VOICES` 64, `MAX_EVENTS` 256 (a full array drops new events), `need(chunk)`
  (pool bytes = `VoicePool` + `Wt` tables for capacity × wavetable slots + decoded GSM samples + `Chan::need`), `new`
  (parses the chunk, seeds its `Xorshift` from one song-rng draw, builds the `VoicePool` of 1–64 voices from that stream),
  `voices()`, `mod_wheel` (writes `ctl::WHEEL`), `set_param` (clamps, ignores structural/non-finite, applies
  unison/steal/mode pool settings), `quiet_after`.
- `run` → `render` (`run_ext` only with `us-strip`: extra input pairs `ext[8..12]` go through `play`/`render_span` to
  `Voice::render` for aux oscillators): denormal guard, clears main, events at exact frames, `render_span` ticks voices on
  the absolute 8-frame (`FINE`) grid and the pool on the 32-frame (`CONTROL`) grid, renders up to the shortest voice
  segment (`Voice::left`) or the next control frame.
- `reset` = fresh load: restores only `p[CTL..CTL+ctl::LEN]` from `ctl0` (wheel/bend/range, parameters 180–182), not the
  appended `ext` (slicing to the end once panicked and every DAW seek/play trapped).
- `quiet_after` = `Some(0)` only without sounding voices, queued events, channel stage and speech.
- Load checks (`check_modules`, `us-trusted` returns Ok at once; `needs::instrument_why` mirrors them): quirks first
  (`module::quirk_module`), filter types with `Forms::read`, envelopes via `env::check_settings` (envelope 1 and every
  envelope a matrix slot reads), noise source (level set or routed) → `NOISE`. `check_voice`: LFO read by a slot →
  `VOICE_LFO`, glide > 0 or mono + slide > 0 → `VOICE_GLIDE` (before mono), mono → `VOICE_MONO`, unison > 1 →
  `VOICE_UNISON`, one-shot → `VOICE_ONESHOT`, any `modmx::routes` slot → `MATRIX`, `needs_add` → `MATRIX_ADD`, each only
  without its feature. `check_v2`: LFO settings (`LfoP::uses_settings`) → `LFO_FRAME`, drive, parallel routing,
  voice-out settings, combine (`us-osc-combine`), channel stage or aux oscillator → `STRIP` (aux osc without `us-strip`
  refused). Speech at `speech::{ON,SELECT,PITCH}`. `mod voice_gating` tests the voice paths.
- `probe` feature: `set_taps(on)`, `probe()` → `probe::Probe` (sounding voices `Voice::snap` + per active filter slot
  taps), `taps: Option<probe::Ring>` begun/ended per `run`; `play` = events + `render_span(…, base, tap)` (tap args only with
  `probe`).
- See change: add-unified-synth, shrink-unified-synth, add-device-live-view, add-v2-import (5.2), add-quiet-track-skip,
  unify-v2-algorithms 6.5–6.8.

## Former AGENTS.md row (moved, full text)

- `UInstrument` (kind 64 `Device`): `need`/`new` (chunk → `VoicePool` of 1–64 voices, own `Xorshift`), `set_param`, `reset`, `quiet_after`;
- `run`/`run_ext` → `render_span` (8-frame voice tick, 32-frame control grid);
- `check_modules` → `UnsupportedModule` per missing feature;
- `probe` taps.
