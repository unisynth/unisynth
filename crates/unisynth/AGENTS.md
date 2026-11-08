# crates/unisynth

MIT no_std f32 unified synth (change add-unified-synth, archived): one slot-based instrument (`synth-mix` kind 64) and one merged effect set (kinds 65–73) from the WaveSabre and Tunefish algorithms (plus V2, mini-cauldron modes); checked against the original engines in `synth-convert` tests. WaveSabre-derived code keeps the upstream notice (`LICENSE-WaveSabre`). Depends on `synth-core`, `libm`, optional `gsm`, `dls`. Module types are cargo features `us-*` (default `us-all`); a disabled module → `LoadError::UnsupportedModule(id)`; `std` = host-only. Docs: `docs/synth/unisynth.md`. Feature list, toolchain, target checks, `tools/check-unisynth-features.sh`: `FEATURES.AGENTS.md`.

| File | Purpose |
|---|---|
| `Cargo.toml` | Deps `libm`, `synth-core`, optional `gsm`/`dls`; features `us-*` per module (default `us-all`), host-only `std`/`probe`. → see `Cargo.toml.AGENTS.md`. |
| `LICENSE-WaveSabre` | Upstream MIT notice (Copyright 2012-2021 WaveSabre Team) for the WaveSabre-derived DSP; keep it. |
| `examples/render.rs` | (`std`) `render <song.bin> [out.wav] [bank.dls]`: renders a unified song, prints peak + pool size; exit 1/2/3. → see `examples/render.rs.AGENTS.md`. |
| `src/chan.rs` | Channel strip after the voice sum (`us-strip`): `ChanP`, `Chan`, `Sends`; DC filter, compressor, boost, dist/chorus, sends. → see `src/chan.rs.AGENTS.md`. |
| `src/chunk.rs` | Instrument chunk v3 (v4 = + speech text): `Chunk::parse` validates once; `values()`, `pairs()`, `sample*`, `text()`, host `encode*`. → see `src/chunk.rs.AGENTS.md`. |
| `src/chunk/tests.rs` | Chunk tests: canonical round trip, truncation/hostile-size rejection, escapes, GSM index rule, speech text. → see `src/chunk/tests.rs.AGENTS.md`. |
| `src/device.rs` | `UDevice` (`Instrument`, `Effect`); `Kinds` registry (64 instrument, 65–73 effects); `Device` forwarding incl. `quiet_after`/`wake`. → see `src/device.rs.AGENTS.md`. |
| `src/env.rs` | ADSR envelopes at control rate: `Env`, `EnvP` (law seconds/rate/frame, curve), `Stage`; laws gated by `us-env-*`. → see `src/env.rs.AGENTS.md`. |
| `src/flt.rs` | Filter slots: `Kind`, `Forms`, `coefs`/`coefs_steps`, `Slot`, `run`; `check` → `UnsupportedModule` per `us-flt-*`. → see `src/flt.rs.AGENTS.md`. |
| `src/fm.rs` | Elementary functions replacing `libm` in the i686 player: f64 `exp2`/`log2`/`pow`/`sin_cos`, f32 wrappers, `sqrt`. → see `src/fm.rs.AGENTS.md`. |
| `src/fx/chorus.rs` | Chorus (kind 69): 3 params; 6 lines, control every 512 absolute frames (`ctx.frame % 512`), LFO step `50p²·512/rate`, own fixed-seed phases redrawn on reset. |
| `src/fx/comb.rs` | Flanger mode 2 `comb` (mini-cauldron comb + doubler): 4 params; `us-fx-flanger-comb`, module 56. → see `src/fx/comb.rs.AGENTS.md`. |
| `src/fx/compact.rs` | Reverb mode 1 `compact` (Tunefish): 4 params on the Freeverb core. |
| `src/fx/compressor.rs` | Compressor (kind 72, WaveSabre Smasher), `Compressor`: 7 params, 2 ms lookahead, key from main or the sidechain pair (read-only). |
| `src/fx/crusher.rs` | Crusher (kind 66): vertical, horizontal, dry/wet. Bit-identical to WaveSabre Crusher. |
| `src/fx/curve.rs` | Distortion mode 1 `curve` (QM Screwdriver): 8 params, three-segment power curve; `us-fx-distortion-curve`, module 38. → see `src/fx/curve.rs.AGENTS.md`. |
| `src/fx/delay_ms.rs` | Delay mode 1 `ms` (Tunefish): 3 params; times in ms at the song rate (1 s lines), zero-latency copy at gain k. |
| `src/fx/formant.rs` | Formant (kind 73, Tunefish): 2 params, 10-pole vowel filter; at rates ≠ 44.1 kHz the poles are found (Durand–Kerner) and mapped `z → z^(44100/rate)`, DC gain kept. |
| `src/fx/freeverb.rs` | Reverb mode 0 `freeverb` (WaveSabre Cathedral): 8 params, freeze, width, pre-delay, SVF cuts. → see `src/fx/freeverb.rs.AGENTS.md`. |
| `src/fx/golden.rs` | Effect self-goldens: FNV-1a of every mode at 44.1 kHz/120 BPM and 48 kHz/90 BPM vs `tests/golden_fx.txt`. → see `src/fx/golden.rs.AGENTS.md`. |
| `src/fx/law.rs` | Effect laws as in the original engines: `exp2f`, `ms_to_len`, `db_to_scalar`, `param_to_*`, `eq_gain`, `fast_sin/cos`. → see `src/fx/law.rs.AGENTS.md`. |
| `src/fx/mod.rs` | Effect kinds 65–73: `UFx`, `Effect` trait, `modes!` table, `MODES` (22), mode/module lookups, host `encode`. → see `src/fx/mod.rs.AGENTS.md`. |
| `src/fx/parametric.rs` | EQ mode 0 `parametric` (WaveSabre Leveller): 14 params, 5 bands per channel, `fast_sin` coefficients. Bit-identical to the original effect. |
| `src/fx/phaser.rs` | Flanger mode 0 `phaser` (WaveSabre Twister): 9 params, flanger/phaser types, vibrato, spread, SVF cuts; 1 s lines. Bit-identical to the original effect. Allpass `y`/`zm1` through `denormal::flush` (wasm). |
| `src/fx/power.rs` | Distortion mode 2 `power` (Tunefish): `sign·q(|x|)^(1−amount)`, 15-bit quantised. 1 param. |
| `src/fx/reverb.rs` | `Combs` core (Freeverb comb/allpass network) shared by freeverb and compact; sleeps after `SLEEP` silent frames. → see `src/fx/reverb.rs.AGENTS.md`. |
| `src/fx/ring.rs` | Flanger mode 1 `ring` (Tunefish): 4 params; the reference's bouncing-counter sweep with phase advance and 0.1–12.1 ms range in seconds at the song rate. |
| `src/fx/rooms.rs` | Reverb mode 2 `rooms` (WaveSabre Chamber): 6 params, 8 cross-fed lines (3–31 ms × room 1/5/10). |
| `src/fx/shaper.rs` | Distortion mode 0 `shaper` (WaveSabre Scissor): 6 params drive, threshold, foldover, dry/wet, shaper, oversampling. Bit-identical to the original effect. `quiet_after` 0. See change: add-quiet-track-skip. |
| `src/fx/svf.rs` | WaveSabre Chamberlin SVF (2× oversampled, `1.5·fast_sin`) as low/high cut in the tempo, phaser, freeverb and rooms modes. `low`/`band`/`high` through `denormal::flush` (wasm). |
| `src/fx/tempo.rs` | Delay mode 0 `tempo` (WaveSabre Echo): 9 params, tempo-synced lines; bit-identical to the original. → see `src/fx/tempo.rs.AGENTS.md`. |
| `src/fx/tests.rs` | Effect test harness (`with_fx`, `run`, `chunk`) + per-mode tests: spot values, rate, call-size independence, quiet-skip. → see `src/fx/tests.rs.AGENTS.md`. |
| `src/fx/three_band.rs` | EQ mode 1 `3band` (Tunefish): 880 Hz / 5 kHz corners at the song rate, 3 gains + `pan`, ramped changes. → see `src/fx/three_band.rs.AGENTS.md`. |
| `src/fx/v2.rs` | V2 global effects as modes: EQ `cut`, delay `v2`, reverb `v2`, compressor `v2`; port of V2 after v2redux. → see `src/fx/v2.rs.AGENTS.md`. |
| `src/fx/wrap.rs` | Distortion mode 3 `wrap` (mini-cauldron 8-bit wrap-around): drive, offset; `us-fx-distortion-wrap`, module 55. → see `src/fx/wrap.rs.AGENTS.md`. |
| `src/instrument.rs` | `UInstrument` (kind 64 `Device`): `need`/`new` (chunk → `VoicePool`), `set_param`, `run`/`run_ext` → `render_span`, `probe` taps. → see `src/instrument.rs.AGENTS.md`. |
| `src/instrument/tests.rs` | Instrument tests: onset, stealing, unison, mono/glide, envelopes, LFO, matrix, call-split independence, clocked, quiet. → see `src/instrument/tests.rs.AGENTS.md`. |
| `src/instrument/tests/flt_noise.rs` | Noise + filters through the voice: level law, decorrelation, filter types/slots, cutoff/resonance routes. Needs `us-osc-pm`, `us-noise`, `us-flt`. → see `src/instrument/tests/flt_noise.rs.AGENTS.md`. |
| `src/instrument/tests/golden.rs` | Self-goldens: FNV-1a 64 of 35 synthetic patches vs `tests/golden.txt`; same output at call sizes 512 and 77. → see `src/instrument/tests/golden.rs.AGENTS.md`. |
| `src/instrument/tests/osc_blit.rs` | Saw/pulse (blit): spectrum, polarity, per-slot pitch, leak corner, phase persistence, start-phase spread. Needs `us-osc-blit`. → see `src/instrument/tests/osc_blit.rs.AGENTS.md`. |
| `src/instrument/tests/osc_pm.rs` | Phase-mod operator: pure sine, Square35 blend (1/3, 1/5 harmonics), two-operator FM against a Bessel reference, feedback, Falcon ratio/ratio fine, gain/mix/octave/fixed Hz, missing-module load error. |
| `src/instrument/tests/osc_sample.rs` | Sampler: pitch laws, interpolation, loops, GSM decode, one-shot, call-split/reset independence; builds a tiny DLS bank. → see `src/instrument/tests/osc_sample.rs.AGENTS.md`. |
| `src/instrument/tests/osc_dls_instrument.rs` | DLS-instrument source: synthetic multi-wave bank (splits, layers, loops, banks) + real macOS bank when present. → see `src/instrument/tests/osc_dls_instrument.rs.AGENTS.md`. |
| `src/instrument/tests/osc_wt.rs` | Wavetable: partial series, pitch at rates, detune/spread/drive, rebuild cadence, PM send, pool sizing. Needs `us-osc-wt`. → see `src/instrument/tests/osc_wt.rs.AGENTS.md`. |
| `src/instrument/tests/pool.rs` | Memory: `need` is a safe bound, `OutOfMemory` one byte under, shared GSM decode, hostile sample sizes → `BadChunk`. → see `src/instrument/tests/pool.rs.AGENTS.md`. |
| `src/instrument/tests/probe.rs` | Probe: env 1 sustain stage/level + age, steal → new id, release flag, LFO phase = rate × time, sampler position advances, cutoff Hz + low-pass taps lose energy, taps leave output bit-identical. |
| `src/instrument/tests/v2.rs` | V2 scenarios: pulse harmonics, ring mul, parallel balance, voice distortion, channel stage, `mix_*` V2 filters/envelopes. → see `src/instrument/tests/v2.rs.AGENTS.md`. |
| `src/law.rs` | Normalized → physical laws (all clamp `p`, NaN → 0): `cutoff`, `gain`, `lfo_rate`, `time`, `transpose`, `pan`, wavetable, start phase; host `inv_*`. → see `src/law.rs.AGENTS.md`. |
| `src/lfo.rs` | One LFO for every law: `Lfo`, `LfoP::read`, `shape::*` (8), `sync::*`, `scale_view`/`offset_view`. → see `src/lfo.rs.AGENTS.md`. |
| `src/lib.rs` | Crate root: `kind` ids (64 instrument, 65–73 effects), feature-gated modules, re-exports `UDevice`, `UInstrument`; `no_std`. → see `src/lib.rs.AGENTS.md`. |
| `src/migrate.rs` | Host-only old layout → v3 migration: `values`, `instrument_chunk`, `lane`, `effect_chunk`, `encode_old*`, `MigrateError`. → see `src/migrate.rs.AGENTS.md`. |
| `src/migrate/crosscheck.txt` | `research/unify-v3-layout/crosscheck.py sample`: 111 instruments (one per format version and feature) as old pairs | v3 pairs (f32 bits); the migration test compares them. Generated; do not edit. |
| `src/migrate/tests.rs` | Migration tests: reference match (`MIGRATE_CROSSCHECK` = full), frozen layout = `v3-layout.tsv`, rename map, lanes, effects. → see `src/migrate/tests.rs.AGENTS.md`. |
| `src/modmx.rs` | Mod matrix: `Matrix`, `source::*`, `op::*`, `target::*`, `Query` (`Matrix` scan, `Tab` one pass); 16 slots with `us-matrix-add`. → see `src/modmx.rs.AGENTS.md`. |
| `src/modmx/tests.rs` | Matrix tests: neutral, Scale/Offset/Add semantics, source values, LFO views, slot-amount modulation, target table, gating. → see `src/modmx/tests.rs.AGENTS.md`. |
| `src/module.rs` | Module ids for `UnsupportedModule` (never renumbered); one `modules!` table: consts, `ALL`, `feature`, `compiled`, `name`. → see `src/module.rs.AGENTS.md`. |
| `src/needs.rs` | Host (`std`): modules a song needs: `instrument(chunk, reach)`, `instrument_why`, `effect(kind, chunk)`, `feature(id)`. → see `src/needs.rs.AGENTS.md`. |
| `src/noise.rs` | Noise source: `Noise` (per-voice xorshift, `start`, `next(level, band, width)`), band = 4-pole LP then RBJ HP. → see `src/noise.rs.AGENTS.md`. |
| `src/osc.rs` | Oscillator slots: `Kind` (`Pm`, `Blit`, `Sampler`, `Wt`, `V2`, `Clocked`), `Slot`, `sample`, `mix_in`, clocked generator. → see `src/osc.rs.AGENTS.md`. |
| `src/osc/cauldron_filter_vectors.rs` | Generated mini-cauldron filter reference `CASES` (5); `include!`d by `osc.rs` tests. Do not edit. → see `src/osc/cauldron_filter_vectors.rs.AGENTS.md`. |
| `src/osc/cauldron_vectors.rs` | Generated mini-cauldron oscillator reference `CASES` (400 outputs each); read by `osc.rs` clocked tests. Do not edit. → see `src/osc/cauldron_vectors.rs.AGENTS.md`. |
| `src/params.rs` | The frozen v3 layout, 294 values: block bases (`v`, `o`, `f`, `e`, `l`, `MOD`, `ch`, …), plain choices, `DEFAULTS`. → see `src/params.rs.AGENTS.md`. |
| `src/params/host.rs` | Host-side v3 `Law` (`map`, `inverse`), `law_of`, `name`; compiled out of the player. → see `src/params/host.rs.AGENTS.md`. |
| `src/params/old.rs` | Frozen old layout (260 values, chunk v1/v2), host-only: constants, `kinds`, `era`, `DEFAULTS`; read by migration/converters. → see `src/params/old.rs.AGENTS.md`. |
| `src/params/old/host.rs` | Host-only metadata of the frozen old layout: `Law`, `law_of(i)`, `name(i)`. → see `src/params/old/host.rs.AGENTS.md`. |
| `src/params/rename.rs` | `OLD: [(old, new); 260]`: old-layout name → functional name by old index (`""` = removed; envN.slope maps by mode in the migration). Host-only. See change: unify-v2-algorithms. |
| `src/params/tests.rs` | Layout, names, laws, defaults, inverse round trips, rename-map coverage, `the_player_reads_no_format_version`. → see `src/params/tests.rs.AGENTS.md`. |
| `src/probe.rs` | Feature `probe` (host only): `VoiceSnap`, `Probe { voices, taps }`, `cutoff_hz`, `Ring`; read-only. → see `src/probe.rs.AGENTS.md`. |
| `src/ramp.rs` | `Ramp`: linear control value over a segment: `to(target, frames)` starts from the previous end (no drift), `retarget` mid-segment, `set`, `next`. |
| `src/rng.rs` | `Xorshift` (xorshift32, implements `synth_core::rng::Rng`): `new(seed)` (0 → constant), `seed_from(&mut dyn Rng)` (one draw), `next_u32`. |
| `src/smp.rs` | Sampler slot (port of WaveSabre `SamplePlayer`): `Source`, `Sample`, `Loop`, `Interp`, `State`, `pool_need`/`load`, `pick`. → see `src/smp.rs.AGENTS.md`. |
| `src/speech.rs` | `Ronan` (`us-speech`), port of V2 `ronan.cpp`: phoneme/syllable tables, `new`, `tick`, `process`, `note_on/off`, `set_ctl`. → see `src/speech.rs.AGENTS.md`. |
| `src/tables.rs` | Once-init tables: 4096-point sine (`sin_turns`, `sin_at`), wavetable partial profile + phase vector with `us-osc-wt`. → see `src/tables.rs.AGENTS.md`. |
| `src/testing.rs` | Test helpers: `song(...)` blob writer, `Track`, `with_player`, `render` through `synth_core::mix::Player<UDevice, Xorshift>`. |
| `src/v2.rs` | V2 numerics (V2 after v2redux): `fastsin`/`fastatan`, `calcfreq`, `Lrc`, `Dcf`, `Dist`, `frandom`, `Opt<T>`. → see `src/v2.rs.AGENTS.md`. |
| `src/v2/tests.rs` | Unit tests of `v2.rs`: `fastsin` / `fastsinrc` within 2e-4 / 3e-4 of `libm::sinf` (V2's bug below −π/2 kept), fixed-point helpers, kept copies of the earlier LRC and Moog filters (low-pass, split), distortion modes. |
| `src/v2fx.rs` | Shared V2 effect bodies: `Comp`, `Boost`, `ModDel`; used by `fx/v2.rs` and `chan.rs`. → see `src/v2fx.rs.AGENTS.md`. |
| `src/voice.rs` | `Voice<'a>` (`synth_core::voice::Voice<NoteCx>`): per-32-frame control state, `Ramp`s, `tick`/`render`, `VoiceSet`, `Declick`. → see `src/voice.rs.AGENTS.md`. |
| `src/wt.rs` | Wavetable tables (`us-osc-wt`, Tunefish generator): `Shape`, `Table`, `Wt`, `band_bins`, `harmonics`. → see `src/wt.rs.AGENTS.md`. |
| `tests/golden.txt` | Self-golden table of `src/instrument/tests/golden.rs`: `<patch> <FNV-1a 64> peak=<x>`; `UPDATE_GOLDEN=1` rewrites. → see `tests/golden.txt.AGENTS.md`. |
| `tests/golden_fx.txt` | Self-golden table of `src/fx/golden.rs`: `fx<kind>.<mode>@{44k120,48k90} <FNV-1a 64> peak=<x>`, every effect mode on noise + sine (12 000 frames) at 44.1 kHz / 120 BPM and 48 kHz / 90 BPM. Same `UPDATE_GOLDEN=1` rewrite. |
