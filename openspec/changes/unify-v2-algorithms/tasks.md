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
- [x] 5.2 Replace `v2.voice` with voice settings (amplitude source, pan stage, output DC filter, voice law; keysync is `voice.keysync`), and remove the `era` parameter from the player. The remaining era behaviour becomes quirk candidates (`boxed_osc`, `lcg_noise`, `integer_fm`, `exact_math`, `fixed_seed`; design D2) or converter writes (design D3). Verify: no `era(` reads remain in `crates/unisynth/src` outside `era_settings` (a test greps for them); "Pan after distortion" test. ("Quirk disabled" moves to 6.1/7.1 with the quirk parameters, decision C)
- [x] 5.3 Update `needs`, features (quirk features: with the quirk parameters at 6.1, decision C), converters, song tables and the voice / V2 sections of `docs/synth/unisynth.md`. Verify: D7 gates 1–7, results recorded in design.md

## 6. Format and migration

Revised after the doubt-driven review of 2026-11-01 (design "Format-switch review"). 6.1–6.4 come before the layout
is frozen; nothing old is rejected until 6.8.

- [ ] 6.1 Merge `main` into this branch (the 28 + 347 released songs, `tools/v2-extract/make-demos.py`, the DAW
  rewrite, converter changes). Verify: workspace tests and both parity reports pass; inventory counts match the
  design's "Song inventory"
- [ ] 6.2 Baseline renders of every tracked song (`project/music/**`, patches as a one-note phrase) with the current
  build, stored outside the repo as the drift reference. Verify: one render per inventory song, listed with its hash
- [ ] 6.3 Quirk survey before the freeze (was 7.1): force each `EraSettings` quirk candidate off in turn (one-line
  override in `era_settings`), run the V2 parity set and the corpus, write the quirk table (flag, what it reproduces,
  source, format versions, songs that set it, songs outside tolerance without it) to `docs/synth/unisynth.md` and
  `research/v2-synth.md`. Drop every quirk no song needs from the layout (design D2). Verify: "Quirk table"
  scenario; each kept quirk has at least one song outside tolerance without it
- [ ] 6.4 Freeze the v3 layout: `DEFAULTS[COUNT]`, every value rule (including combine replace for FM/aux, the
  oscillator law from the slot kind, raw-encoded polarity/one-shot/sync/routing/keysync, the Cut EQ DC filter) and
  the lane fan-out rule, then count stored pairs per tracked song under the rules (script) and swap placements that
  grow a reference song. Verify: pair-count table in design.md; no reference song stores more pairs than now unless
  recorded as a trade-off
- [ ] 6.5 Frozen old layout, host-side (`crates/song` or `unisynth::old`): old `COUNT`, `DEFAULTS`, decoders, indices,
  `era_settings` over the old layout, old effect modes. The migration `old → new` for an instrument chunk, an effect
  chunk, lanes (one lane per changed setting; a dual law+slope lane or a lane/route on a naive slot's mode or ring is
  a named error) and mod routes. Verify: unit tests per value rule; round trip old → new → render within tolerance on
  every tracked song (`--drift` against 6.2)
- [ ] 6.6 Song file format `song/2` and `us-patch/2`: `song/1` and `us-patch/1` files migrate on load (values and
  names; names via `rename.rs`), are written as `/2`; `/2` is never migrated again. The DAW migrates on open through
  synth-wasm (not only on export); its templates, fixtures, `envRate`-style option sniffing and the schema move to
  the new layout. Verify: "Old song file" for every tracked song; a `/1` file opened in the DAW plays the same as its
  migrated `/2`; loading a `/2` twice is identical
- [ ] 6.7 Converters write the new layout directly: `Patch` at the new `COUNT`, `ws`, `tfm`/`tf4m`, `v2m`,
  `xm`/`xm-phrase`, `auto.rs` (reach, steps), `features` / `needs` (new settings, quirk modules, `us-chunk-wide`
  reach), song tables. No converter writes `voice.era`. Verify: converter tests; each converter's output equals the
  migration of its old output (same values)
- [ ] 6.8 Switch the player to chunk versions 3/4 and the new effect mode ids; reject versions 1/2 and old effect
  modes with an unknown-version error; remove the derivation from era/type. The trusted (baked) path stays unchecked
  by design: baked songs come from the same tree (documented). Verify: "Device kind and chunk" scenarios including
  "Old layout rejected by the player"; truncation at every byte without a panic (checked build); "Quirk disabled"
- [ ] 6.9 Migrate or re-convert every song in the inventory and regenerate size and speed tables. Load-all check
  over the tracked inventory (fails on a song that does not load, is skipped, or a location with fewer songs than
  recorded); external and git-ignored corpora are re-converted by their tools and checked when present (opt-in,
  reported). Verify: load-all ok; "No song left behind"; drift report per tracked song against 6.2 (flags listed for
  listening, D7.3); parity 49/49 and V2 18/18; bake check; `check-song-features.sh --minimal`; size and speed in
  `tools/unisynth-parity/size.md` and design.md

## 7. Quirk documentation and integration

- [ ] 7.1 Document the kept quirks (table from 6.3) with their cargo features and module ids; the source-name ban
  also covers "farbrausch". Verify: "A quirk is listed with its songs"
- [ ] 7.2 Integration check: workspace tests, `tools/check-unisynth-features.sh`, bake check, both parity reports,
  total drift against the task 1.1 baseline per song; AGENTS rows for every touched file. Verify: all pass;
  `openspec validate unify-v2-algorithms --strict` passes

## Workflow follow-up

- User A/B listen of the songs listed by the drift thresholds (D7.3), a few migrated DAW songs and V2 songs, before archive.
- Archive the change and sync specs.
