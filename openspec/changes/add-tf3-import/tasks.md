# Tasks

## 1. Functional spec and reference

- [x] 1.1 Write `research/tunefish3-functional-spec.md` from the Enigma-Studio-3 synth source (behaviour only, no copied code, tables or identifiers): constants, signal flow, point spline (solve and segment formulas), PAD table (profiles, IFFT, playback), oversampling rule and decimation filter, noise, filters, ADSR, LFO, matrix rule, `TF_PARAM_VALUERANGE`, effects, `ePLAYER` exclusions, song format and timing (`.e3prj` XML and binary stream), quirks, and a "differs from TF4" note per section. Verify: every parameter of the 129 has a law entry; every open question of design.md is answered or listed
- [x] 1.2 Feature survey of Transplant, `default.e3prj` and the 128 presets (`tools/tf3-parity/survey.ts`, reading the external checkout): modules, interpolation modes, PAD profiles, matrix sources and targets, effects, editor-only modules in use. Append it to the spec. Verify: the survey runs and skips cleanly without the checkout
- [x] 1.3 `tools/tf3-parity/build-ref.sh` + MIT shim header + `render-ref.cpp` (design D2): pinned clone into `$TF3_REF`, player and editor configurations, `.e3prj` → WAV. Verify: Transplant renders without errors; a one-note synthetic project renders a non-silent WAV; the script exits 0 with a message when the checkout is absent
- [x] 1.4 `tools/tf3-parity/captures.sh` (design D2a): pinned video ids, `uvx yt-dlp -x` → 44.1 kHz WAV in `$TF3_CAPTURES`, checksums and music-start times printed; plus `capture.ts` (windowed alignment, normalised bands 63 Hz–12.5 kHz, onsets, per-window lag) on `tools/synth-parity/metrics.ts`. Verify: the three captures are fetched; the reference render of Transplant (1.3) against its capture gives a stable lag across windows; both skip cleanly when absent
- [x] 1.5 `tools/AGENTS.md` row, `tools/tf-extract/README.md` (Tunefish 3 now importable from projects; executable extraction still unsupported). Verify: `kb_search "tunefish 3 parity"` returns them

## 2. Storage

- [x] 2.1 Pick the expanded oscillator-type choice (design D5) and the appended point-oscillator block (design D3), and implement them in `params.rs` + host names, laws and defaults. Run `doubt-driven-review` and record the outcome in design.md D3/D5. Verify: "Existing songs unchanged" (self-goldens, effect goldens, `research/us-bake-check/check.sh`), "Round trip", `params` tests

## 3. Oscillator types

- [x] 3.1 `us-osc-points`: cycle builder (D3) on the `Wt` playback path, per-voice table, change test, matrix shape targets. Verify: "Step cycle", "Modulated point", "Missing module"; per-mode parity on synthetic projects (step, linear, spline; 1–4 points; unison; sub-oscillator). Done: own pool item `Pts` (11 layers, sum-then-clip, rounding index: the `Wt` path differs in all three); cycle parity with the original sample by sample (`pts::parity`, 7 cases, < 1e-4, opt-in `cycle-ref`); the audio parity of the synthetic projects runs with the converter in 5.1
- [x] 3.2 TF3 decimation stage for `points` and `pad` (k from the note, ≤ 8; two 12.83 kHz ladders at k·rate, also at k = 1; average). Verify: "High note", "Low note still filtered"; speed report row for an 8× slot (`performance-optimization`: measure before tuning)
- [x] 3.3 `us-osc-pad`: load-time table build (generalised IFFT, wavetable path bit-identical), shared per slot and instrument, `need`, structural values, playback (D4). Verify: "Spectrum", "Automation ignored", "Need with 16 voices"; per-profile parity; load time of a 2^16 table measured
- [x] 3.4 Gating: `module.rs`, `needs.rs`, `check_modules`, feature list in the converter, `tools/check-unisynth-features.sh`. Verify: the script passes; i686 core-only and wasm32 checks pass
- [x] 3.5 Docs: `docs/synth/unisynth.md` oscillator sections (diagrams), Memory table, `crates/unisynth/AGENTS.md` rows. Verify: the Mermaid blocks parse

## 4. Converter

- [x] 4.1 `crates/synth-convert/src/e3prj.rs`: XML reader (songs, patterns, pattern instances, instruments by parameter name), errors (design D7), plus a test writer for synthetic projects. Verify: "Missing pattern"; truncation and unknown-name tests without a panic
- [x] 4.2 `src/tf3.rs`: instrument mapping (design D6) reusing `tf.rs` where the laws agree; player and editor modes; warnings. Verify: "Player mode drops chorus", "Sub-oscillator"; per-module parity on synthetic projects (runs with the song converter in 5.1; LFO shapes proven equal to Tunefish 3 values by `lfo_shapes_keep_tunefish_3_values`)
- [x] 4.3 Song mapping (design D7): lanes, block grid, velocity −1 and instrument −1 rules, half-row-early timing, parameter quantisation, column release, effect words → controllers, master scaling; CLI `e3prj` and `tf3`. Verify: "Note without instrument", "Note-off", "Timing", "Transplant converts", "Preset"; integrated loudness of Transplant within 1 dB of the reference (measured: 0.06 dB, octaves ≤ 0.93 dB, timing lag 0; vs the capture ≤ 1.13 dB)
- [x] 4.4 Docs: `docs/synth/unisynth.md` converter section ("Tunefish 3 projects", walkthrough lines), new `docs/synth/tunefish3.md`, `crates/synth-convert/AGENTS.md`. Verify: `kb_search "e3prj import"` finds them

## 5. Parity

- [x] 5.1 `tools/unisynth-parity` `tf3` group: synthetic items and Transplant, player mode, onsets within 384 frames; committed report (metrics only). Add the capture check of converted Transplant ("Tunefish 3 capture check"). Fix gaps with `systematic-debugging`; record known differences (shared spline, per-slot drive, per-instrument mod inputs) with their measured effect. Verify: "Transplant", "Checkout absent", "Transplant capture", "Capture absent". Done: 30 of 30 items pass (`tf3-report.md`; PAD items within the original's own measured run-to-run variance); also the audio parity that 3.1 and 4.2 deferred
- [x] 5.2 "Repository check": no Enigma Studio 3 source, project, preset or render is tracked. Verify: `git ls-files` scan in the parity tool's test

## 6. Released songs and DAW editing (user request after 5.2)

- [x] 6.1 `tools/tf3-extract`: `unpack.py` emulates the kkrunchy stub (unicorn) until the intro's first API call and dumps the image; `extract.py` finds the synth block (32 instrument flags, parameter-major bytes in a ranged or percent variant, the song) and writes an `.e3prj`. Verify: Transplant's executable converts byte-identically to its source-release project; Train Control found (percent variant, 15 instruments)
- [x] 6.2 `tools/tf3-extract/make-demos.py` writes `project/music/released/{transplant,train-control}.song.json` with meta and parity; NOTICE.md lists them ("Released Tunefish 3 intro songs"). Verify: both load (`song check`), render within 0.07 dB loudness and 1.02 dB per band of the original player, within 1.40 dB per band of the captures, timing 7/7 and 5/5 windows
- [x] 6.3 DAW: Points slot block (a column per point: offset, value, mode of the segment ending there; last segment; count, drive, layers knobs; screen draws `pts::cycle` with point marks) and PAD table block (bandwidth, scale, damp, harmonics, profile by name, drive; screen sketches the partials). A PAD slot's table inputs are load-time (`params::is_load_time`): the live host rebuilds the device, the DAW pushes on knob release and offers no automation for them. Verify: params.test.ts (cycle = engine fixtures, load-time rules), device.test.ts (points grid follows the count, PAD profile name), synth-wasm live.rs (PAD input → rebuild, drive → parameter set)

- [x] 6.4 Editor-mode fidelity for the presets (user decision: a Tunefish 3 band-pass in the engine first): `filtN.form` 1 on RBJ/notch = Tunefish 3's form (shelf-slope band-pass, RBJ notch `Kind::Tf3Notch`, high-pass resonance to 0.95; `us-flt-feedback`; doubt review: the form field instead of an expanded type choice, which lanes would shift; notch forms one state family), chorus kind 69 mode 1 one tap (`us-fx-chorus-tf3`, module 65), MOD1–4 start at 1; render-ref parses subnormal values. Verify: flt/chorus3/needs/tf3 unit tests, feature gate, bake check, songs unchanged, `tf3-presets.ts` (all within 1.41 dB, 39 of 53 within 3 dB per band over 16 notes)
- [x] 6.5 Presets in the library: `examples/library.rs` `tunefish3` (53 distinct, editor mode), `/api/library` source, DAW "Tunefish 3 presets". Verify: music.test.ts, library.test.ts, all 53 render non-silent
- [x] 6.6 Publish: `scripts/build-daw.sh`, `scripts/build-songs.sh`, commit this change's files and the site, push

## Workflow follow-up

- [ ] Ask the user about the editor-only modules if the survey (1.2) finds Transplant using them (bring the capture comparison of both modes), and about adding the 128 presets to the DAW library
- [ ] Follow-up change: Pandora (Revision 2011) uses an older Tunefish 3: 21 instruments of 100 bytes stored instrument-major (the first 98 match parameters 0–97 of the known list, then one value and the instrument index; no effect slots) and a different song encoding (1-byte name length; patterns not in the 5-byte cell layout). Needs that engine's parameter list and effects reverse-engineered from the executable; its capture (D2a) is the acceptance reference
