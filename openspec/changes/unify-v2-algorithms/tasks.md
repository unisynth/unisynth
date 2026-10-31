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

- [ ] 5.1 Add the combine op (add, multiply, replace) to every oscillator slot kind, and move the V2 generators onto the shared slot path with the oscillator law (transpose, detune, colour, volume). Verify: "Ring on a band-limited slot" and the V2 oscillator tests pass; the `mix_*` tests pass
- [ ] 5.2 Replace `v2.voice` with voice settings (amplitude source, pan stage, output DC filter, voice law; keysync is `voice.keysync`), and remove the `era` parameter from the player. The remaining era behaviour becomes quirk candidates (`boxed_osc`, `lcg_noise`, `integer_fm`, `exact_math`, `fixed_seed`; design D2) or converter writes (design D3). Verify: no `era(` reads remain in `crates/unisynth/src` (a test script greps for them); "Pan after distortion" and "Quirk disabled" tests
- [ ] 5.3 Update `needs`, features (quirk features), converters, song tables and the voice / V2 sections of `docs/synth/unisynth.md`. Verify: D7 gates 1–7, results recorded in design.md

## 6. Format and migration

- [ ] 6.1 Switch the player to chunk versions 3/4 in the new layout and reject versions 1/2. Verify: the "Device kind and chunk" scenarios, including "Old layout rejected by the player", and truncation at every byte without a panic
- [ ] 6.2 Add the host migration in `crates/song` (old chunk versions and old song-file names → new settings) and apply it on import. Verify: "Old patch sounds the same" and "Old song file" for every song in `project/music/` and `project/music/patches/`, and for the V2 acceptance songs written as old-layout chunks (`--drift` within the D7.3 thresholds against the baseline renders)
- [ ] 6.3 Re-convert or migrate the repo songs (`project/music/*.song.json`, `research/us-size-probe/out/songs/*.song`) and regenerate size and speed tables. Verify: bake check ok; `check-song-features.sh --minimal` passes; size and speed recorded in `tools/unisynth-parity/size.md` and design.md

## 7. Quirk documentation

- [ ] 7.1 Add the quirk survey (`tools/v2-extract` or `synth-convert features --quirks`) over the corpus and acceptance set. It renders each song that sets a quirk with the flag cleared, and writes the quirk table (flag, what it reproduces, source, format versions, songs that set it, songs that leave tolerance without it) to `docs/synth/unisynth.md` and `research/v2-synth.md`. Verify: "Quirk table" scenario: every flag the converter can set appears
- [ ] 7.2 Delete every quirk no song needs (design D2): its flag, feature, code and converter write. Verify: the survey re-run lists only kept quirks, each with at least one song outside tolerance without it; the V2 parity report passes
- [ ] 7.3 Integration check: workspace tests, `tools/check-unisynth-features.sh`, bake check, both parity reports pass, total drift against the task 1.1 baseline listed per song; AGENTS rows for every touched file. Verify: all pass; `openspec validate unify-v2-algorithms --strict` passes

## Workflow follow-up

- User A/B listen of the songs listed by the drift thresholds (D7.3), a few migrated DAW songs and V2 songs, before archive.
- Archive the change and sync specs.
