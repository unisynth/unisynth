# Tasks

## 1. Baseline and layout

- [x] 1.1 Record the baseline in `tools/unisynth-parity/unify-baseline.md`: the parity reports (`parity.ts`, `v2.ts`), the renders of every parity item kept as WAV (git-ignored) as the drift reference, self-goldens, bake check, the size table (`measure.sh` with V2_LIST, plus debris and platinum) and the speed report. Add a `--drift <dir>` option to `parity.ts` and `v2.ts` that compares each render with the stored one (loudness Δ, worst band Δ) and flags D7.3 thresholds. Verify: `--drift` on the unchanged tree reports 0.00 dB for every item
- [x] 1.2 Design the new parameter layout (design D4) and fix the functional names (design D8): per filter slot (topology, response, resonance form, oversampling form, feedback clip, law), per envelope (law, curve, sustain slope, amplify), per LFO (shape union, polarity, one-shot, phase, law), per oscillator slot (combine, law, keysync), voice (amplitude source, pan stage, DC filter), channel strip, drive and the quirk candidates (design D2). Write it as a table in design.md (D4 results). Run `doubt-driven-review` on it and record the outcome. Verify: the table covers every parameter of `params.rs` (a test lists old names → new names or "removed" with no gaps)
- [x] 1.3 Rename to the functional names with no behaviour change: parameter names and option values (`params/host.rs`, song tables), cargo features and module ids (`Cargo.toml`, `module.rs`, `needs.rs`, `tools/check-unisynth-features.sh`, `marginal.sh`, `check-song-features.sh`), effect mode names, docs sections, plus a provenance table in `docs/synth/unisynth.md`. The old-name map goes into the song import (first part of 6.2). Verify: the "No source names in the parameter table" test; `tools/check-unisynth-features.sh` passes; `--drift` 0.00 dB on every item

## 2. Filters

Stages 2–5 change engine code only (design D4, "Order of work"): settings are derived from today's type and mode
values, and new parameters, converter writes and song tables move to 6.1/6.2. "Forms combine freely" and similar
combination scenarios are engine tests until 6.1 and patch tests there.

- [x] 2.1 Merge the state-variable filters in `flt.rs` into one formula (design D1): response (all-pass for every form), resonance form, oversampling form and bias as coefficients, the law at control rate. Remove the separate V2 filter kind and `v2::Flt`'s LRC use in filter slots. Verify: new tests "Forms reproduce the earlier filters" (octave bands within 0.5 dB, cutoff within 3 % against the old code kept in the test) and "Forms combine freely"; the existing filter scenario tests pass
- [x] 2.2 Merge the ladder and the V2 Moog into one 4-pole formula (response, feedback clip before/after, 1×/2×, law, bias). The bypass before v6 moves to the converter (design D3). Verify: "Shared 4-pole filter" scenarios (within 0.5 dB per band against the old code); the 4-pole response test; `tests/engines.rs` ladder checks within their tolerance
- [x] 2.3 Update `needs`, `module.rs` features (design D5), `synth-convert` (`ws`, `tf`, `v2conv` write the settings; feature list), song tables, and `docs/synth/unisynth.md` (filter section). Verify: D7 gates 1–7 (tests, parity within tolerance, drift recorded, goldens regenerated, bake, size, speed), the results recorded in design.md under "Stage results"

## 3. LFOs

- [x] 3.1 Merge the LFO modes in `lfo.rs` into one LFO (one phase domain): shape union, polarity, one-shot, start phase and rate law (`hz`, `frame`) on every LFO. Verify: the LFO scenario tests pass; new "Sample-and-hold on a Hz-law LFO" test; the old-mode settings stay within 0.5 dB / one control block of the old code (test)
- [x] 3.2 Update `needs`, features, converters, song tables and the LFO section of `docs/synth/unisynth.md`. Verify: D7 gates 1–7, results recorded in design.md

## 4. Envelopes

- [x] 4.1 Merge the envelope modes in `env.rs` into one stage machine (design D1): linear attack; decay and release curve (quadratic, power, exponential); sustain slope; amplify; time law (`seconds`, `rate`, `frame`); `coarse_env` as a quirk candidate. Verify: the envelope scenario tests pass; "Sustain slope on a seconds-law envelope" and "Earlier modes as settings" (within one control block and 0.5 dB above −60 dB of the old code for each mode)
- [x] 4.2 Update `needs`, features, converters, song tables and the envelope section of `docs/synth/unisynth.md`. Verify: D7 gates 1–7, results recorded in design.md

## 5. Oscillator combine and voice settings

- [x] 5.1 Add the combine op (add, multiply, replace) to every oscillator slot kind, and move the V2 generators onto the shared slot path with the oscillator law (transpose, detune, colour, volume). Verify: "Ring on a band-limited slot" and the V2 oscillator tests pass; the `mix_*` tests pass
- [x] 5.2 Replace `v2.voice` with voice settings (amplitude source, pan stage, output DC filter, voice law; keysync is `voice.keysync`), and remove the `era` parameter from the player. The remaining era behaviour becomes quirk candidates (`boxed_osc`, `lcg_noise`, `integer_fm`, `exact_math`, `fixed_seed`; design D2) or converter writes (design D3). Verify: no `era(` reads remain in `crates/unisynth/src` outside `era_settings` (a test greps for them); "Pan after distortion" test. ("Quirk disabled" moves to 6.8 with the quirk parameters, decision C)
- [x] 5.3 Update `needs`, features (quirk features: with the quirk parameters at 6.8, decision C), converters, song tables and the voice / V2 sections of `docs/synth/unisynth.md`. Verify: D7 gates 1–7, results recorded in design.md

## 6. Format and migration

Revised after the doubt-driven review of 2026-11-01, cycles 1 and 2 (design "Format-switch review"). 6.1–6.4 are
separate commits; 6.5–6.8 are checkpoints of one switch commit (each needs a player that reads v3); 6.9 follows.

- [x] 6.1 Merge `main` into this branch (the 28 + 347 released songs, `tools/v2-extract/make-demos.py`, the DAW
  rewrite, converter changes), applying the 1.3 and clocked renames to main's new code; re-run the corpus lane and route check over every tracked
  song. Verify: workspace tests and
  both parity reports pass; the source-name check (parameter table and the DAW's user-facing strings) passes;
  inventory counts match the design's "Song inventory"
- [x] 6.2 Baseline renders of every tracked song (`project/music/**`; patches and DAW templates as a one-note phrase)
  with the current build, stored outside the repo as the drift reference. Verify: one render per inventory song and
  template, listed with its hash
- [x] 6.3 Quirk survey before the freeze (was 7.1): split the candidates (`integer_fm` apart from `exact_math`, DC bias
  apart from the DC filters), clear each in turn through `era_settings`, render the V2 parity set and every tracked V2
  song. Keep a quirk when clearing it takes a song outside the parity tolerance (against the original render where one
  exists, else against its quirk-on render); songs that only cross the D7.3 thresholds are listed for listening. Write the quirk table (flag, what it reproduces, source, format
  versions, songs that set it, songs that move without it) to `docs/synth/unisynth.md` and `research/v2-synth.md`.
  Verify: "Quirk table" scenario; each kept quirk lists at least one song that moves without it
- [x] 6.4 Freeze the v3 layout: index map with the kept quirks only (renumbered), `DEFAULTS[COUNT]` (curve "follows law", filter
  response the type's own, so reference songs store no new pairs), every value rule (combine replace for FM/aux, the
  oscillator law from the slot kind, raw polarity/one-shot/sync/routing/keysync, 4-pole clip/steps/bias, the Cut EQ DC
  filter, slope by the law at each lane point), the effect-chunk version marker. Gate: a prototype build of the layout
  passes the squishy probe of the reference songs (no growth); pair counts per tracked song recorded. Verify:
  index map and pair-count table in design.md; probe table
- [x] 6.5 (switch commit) Frozen old layout host-side (old `COUNT`, `DEFAULTS`, decoders, indices, `era_settings`
  over the old layout, old effect modes) and the migration `old → new` for instrument and effect chunks, lanes (one
  lane per changed setting at the old lane's times, merged into an existing lane on the same setting; a lane or route
  on a naive slot's mode or ring is a named error) and mod routes. Verify:
  a unit test per value rule; "Lanes across a split"
- [x] 6.6 (switch commit) `song/2` and `us-patch/2`: `/1` files migrate on load (values and names) and save as `/2`;
  `/2` is never migrated and rejects old names. The DAW migrates on open through synth-wasm; its templates, fixtures,
  option sniffing, its lane fallback (an unknown lane is an error, not `filt1.cutoff`) and the schema move to the new
  layout. Verify: "Old song file", "Migrated once", "New file with an old name"; a `/1` file opened
  in the DAW plays the same as its migrated `/2`
- [x] 6.7 (switch commit) Converters write the new layout directly: `Patch` at the new `COUNT`, `ws`, `tfm`/`tf4m`,
  `v2m`, `xm`/`xm-phrase`, `legacy`, `instr`, `fx`, `tf`, `cauldron`, `capacity`, `rust` (bake), `auto.rs`, `features`/`needs` (new settings, quirk modules and features, `us-chunk-wide`
  reach), song tables. No converter writes `voice.era`. Verify: each converter's output equals the migration of its
  old output
- [x] 6.8 (switch commit) The player reads chunk versions 3/4 and the effect-chunk marker, rejects versions 1/2 and
  unmarked or truncated effect chunks with `LoadError::UnknownVersion` / an error, and no longer derives settings from era/type; quirk
  parameters are structural, each behind its module id and feature; `is_channel` and the other index-order checks
  follow the new strides. The trusted (baked) path stays unchecked by design (documented). Verify: "Device kind and
  chunk" and "Effect chunk version", incl. "Old layout rejected by the player"; truncation at every byte without a panic (checked build); "Quirk
  disabled"; workspace tests, feature builds, bake check, parity 49/49 and V2 18/18; squishy probe and speed against
  6.4 (D7.6, D7.7)
- [x] 6.9 Migrate every tracked song to `song/2` and regenerate the git-ignored binaries (size probe, parity caches,
  library) with the new converters. Load-all check over the tracked inventory (fails on a song that does not load, is
  skipped, or a location with fewer songs than recorded); external corpora re-converted and checked when present
  (opt-in, reported). Verify: load-all ok; "No song left behind"; drift report per tracked song and template against
  6.2 (flags listed for listening, D7.3); size and speed recorded in `tools/unisynth-parity/size.md` and design.md

## 7. Quirk documentation and integration

- [x] 7.1 Document the kept quirks (table from 6.3) with their cargo features and module ids; the source-name ban also
  covers "farbrausch" (song credits exempt). Verify: "A quirk is listed with its songs"
- [x] 7.2 Integration check: workspace tests, `tools/check-unisynth-features.sh`, bake check, both parity reports,
  drift per song against the 1.1 baseline (parity items) and 6.2 (every tracked song); AGENTS rows for every touched
  file. Verify: all pass; `openspec validate unify-v2-algorithms --strict` passes

## Workflow follow-up

- User A/B listen of the songs listed by the drift thresholds (D7.3), a few migrated DAW songs and V2 songs, before archive. Done 2026-11-02 (8 before/after pairs, `synth-lab/unify/listen-switch`): "it seems ok".
- After the merge, in the main checkout: regenerate its git-ignored outputs (library example, `measure.sh`, cauldron
  conversions via `cauldron.ts`) and run `load-all.ts --externals`. Done 2026-11-02: library, size probe, the TBL `songs/`, `parity/` and `compact/`
  conversions regenerated; load-all with every location present: no failures.
- Archive the change and sync specs.
