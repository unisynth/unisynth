# Tasks

## 1. Foundations

- [x] 1.1 Extract the GSM 06.10 decoder, `GsmChunk` and `src/testdata/*` from `crates/wavesabre` into a new no_std crate `crates/gsm`. Make `wavesabre` (`ws-specimen`, `ws-thunder`) depend on it and remove its own copy. Verify: `cargo test -p gsm` passes the libgsm vector tests; `cargo test -p wavesabre` passes with `tests/golden.txt` unchanged; the i686 core-only and wasm32 checks of both crates pass
- [x] 1.2 Scaffold `crates/unisynth` (`#![cfg_attr(not(test), no_std)]`; deps `synth-core`, `gsm`, `dls`, `libm`; the `us-*` features of design D9 with default `us-all`; in `default-members`). Add `LoadError::UnsupportedModule` to `synth-core`. Verify: `cargo test -p unisynth`, `cargo test -p synth-core`, and the i686 core-only and wasm32 checks all pass on the empty crate
- [x] 1.3 Add `params.rs`: the D3 layout (indices, defaults, names) and the D2 laws (legacy laws per module type; new laws for cutoff, pitch, levels, LFO rate and times), with the constants fixed against the D2 coverage and resolution criteria. Verify: unit tests show every new law covers both legacy ranges (WaveSabre vibrato 7–77 Hz, Tunefish LFO 0–8 Hz, cutoffs 20 Hz–20 kHz, legacy envelope times), and that byte steps are no coarser than the legacy laws in the common range
- [x] 1.4 Implement chunk v1 (D3): a bounds-checked decoder, an encoder behind a `std` feature, and a sample section parsed by `crates/gsm`. Run `doubt-driven-review` on the layout and chunk format before any later task depends on them, and record the outcome in design.md D3. Verify: the "Sparse chunk", "Malformed chunk", "Bad entries" and "Index past the section" scenarios, truncation at every byte, canonical-order and trailing-byte rejection, non-finite and out-of-range values, and encode→decode round trip tests
- [x] 1.5 Add rows to `crates/AGENTS.md`, `crates/gsm/AGENTS.md` and `crates/unisynth/AGENTS.md`. Start `docs/synth/unisynth.md` (layout, chunk, laws) and link it from `docs/synth/README.md`. Verify: `kb_search "unisynth chunk"` returns the new rows and doc

## 2. Voice framework, envelopes, modulation

- [x] 2.1 Implement the instrument device (kind 64) and the `UDevice` registry (`Kinds`). Include voice allocation on `synth-core`'s `VoicePool` (capacity 1–64, steal policy, voice unison, poly or mono legato with slide, glide, transpose, velocity sensitivity, one-shot) and the shared sine table. Verify: the "Capacity with stealing", "Mono legato slide" and "Onset frame" scenarios (using a test oscillator stub)
- [x] 2.2 Implement the 32-frame absolute control grid with linear interpolation and mid-block note starts (D5). Add per-device random seeding at load plus per-voice streams (D7) and the denormal guard. Verify: the "Caller and sub-block independence", "Repeat render" and "Reset equals fresh load" scenarios
- [x] 2.3 Implement envelopes in WaveSabre mode (ms law, linear A, quadratic D/R) and Tunefish mode (rates per second, slope, decay-can-rise and sustain chase), plus env1-at-output and the voice end rule. Verify: the "Voice end", "Rate independence" and envelope-1-at-output scenarios (the routed half of "Envelope 1 through routes only" is verified in 2.4, when the matrix exists), and curve spot values against the legacy formulas
- [x] 2.4 Implement LFOs (5 shapes, sync/free, D6 unipolar/bipolar forms) and the 12-slot mod matrix (Scale product, Offset sum in physical units with post-sum clamp, sources, the full target list, own-amount one-block delay, in-order evaluation). Verify: the "Key sync", "Vibrato as an offset", "Scale composition", "Pitch offset", "Mod slot depth" and "Envelope 1 through routes only" scenarios
- [x] 2.5 Document the voice framework, control grid and mod matrix in `docs/synth/unisynth.md`, with Mermaid diagrams checked to render, and update `crates/unisynth/AGENTS.md`. Verify: every Mermaid block parses (mermaid render check) and the AGENTS rows list the new files

## 3. Oscillators and noise

- [x] 3.1 Implement the phase-mod operator slot (sine + Square35 blend, feedback, `pm_out` into the next slot, Falcon ratio law). Verify: the "Two-operator FM" scenario, and that ratio spot values equal the Falcon law
- [x] 3.2 Implement the band-limited saw/pulse slot (integrated BLIT, leak 0.998, waveform blend, pulse width, phases per voice persisting across notes, Slaughter coarse/fine law). Verify: the "Pitch per slot" and "Slot off" scenarios; the saw spectrum falls off as 1/n up to 5 kHz at note 45
- [x] 3.3 Implement the spectral wavetable slot (tables, IFFT, spectrum, phase-stage quirk, 1–10 unison layers, detune, spread, drive, octave, fixed frequency, rebuild checked every 2048 frames on change). Generalise it from 44 100 Hz to `Ctx::rate`. Verify: harmonic-series and rebuild-on-change tests; the same pitch at 44.1 and 48 kHz
- [x] 3.4 Implement the sampler slot (GSM sample-section source, GM.DLS wave source, start, loop off/repeat/ping-pong, sample or manual bounds, reverse, nearest/linear, one-shot). Verify: the "No bank" and "Unity pitch" scenarios, plus loop seam, ping-pong and reverse tests against the libgsm vectors
- [x] 3.5 Implement the noise source (per-voice generator, level, optional band filter). Verify: the "Band-limited noise" scenario
- [x] 3.6 Add self-goldens (`src/instrument/tests/golden.rs`, hashes in `tests/golden.txt`; in-crate so it reuses the chunk encoder without the `std` feature) (FNV-1a of synthetic patches covering groups 2–3, `UPDATE_GOLDEN=1` rewrites them), and document the slot types in `docs/synth/unisynth.md` and `AGENTS.md`. Verify: the golden test passes, changing one law constant fails it, and the Mermaid check passes

## 4. Filters, features, memory

- [x] 4.1 Implement the filter slots: SVF LP/HP/BP/notch (2× oversampled), ladder LP, RBJ HP/BP (log₁₀2 bandwidth law), and resonator notch. All use the unified cutoff law, rate-general coefficients and per-control-block coefficient interpolation. Verify: the "Filter off is bypass" and "Low-pass cutoff" scenarios, analytic |H| at cutoff ±1 octave for each type, and resonance spot values per type against the legacy laws (`param_to_q`/`1 − v`, Tunefish `set`)
- [x] 4.2 Put each module type behind its feature, with `UnsupportedModule` at load. Verify: the "Disabled oscillator type" scenario; `tools/check-unisynth-features.sh` builds each single-module feature set for i686 core-only and wasm32 without warnings
- [x] 4.3 Implement `UInstrument::need(chunk)`, which sizes per-voice state by the slot types in use, plus decoded samples. Verify: the "Exact pool" and "Hostile sample sizes" scenarios for a PM-only, a wavetable and a sampler patch, and the "Automating a structural parameter" scenario; a PM-only patch needs under 1/5 of a wavetable patch's pool (the difference is exactly one table per voice)
- [x] 4.4 Extend the goldens with one patch per filter type and document filters, features and memory in `docs/synth/unisynth.md` and `AGENTS.md`. Verify: goldens pass and the Mermaid check passes

## 5. Unified effects

- [x] 5.1 Implement the `UFx` registry (kinds 65–73, mode as parameter 0, legacy parameter lists after it, per-(kind, mode) features, pool sizing from parameters, tempo and rate, reset, absolute-frame control grids). Verify: the "Disabled mode", "Reset" and "Delay capacity" scenarios
- [x] 5.2 Port distortion (Scissor, Tunefish), crusher, and EQ (Leveller, Tunefish 3-band), rate-general. Verify: transfer-curve and band-gain tests at legacy spot values; each mode's parameters decode in legacy order
- [x] 5.3 Port delay (Echo, Tunefish), chorus (512-frame absolute control grid) and flanger/phaser (Twister modes, Tunefish flanger). Verify: the "Echo parameters", "Tunefish delay at 48 kHz" and "Odd call sizes" scenarios
- [x] 5.4 Port reverb on one Freeverb core with Cathedral and Tunefish configs plus the Chamber mode, the compressor (Smasher, sidechain key), and formant. Verify: the "Mode selection" and "Sidechain key" scenarios; formant F1/F2 per vowel
- [x] 5.5 Add effect self-goldens, document the effect set in `docs/synth/unisynth.md`, and update `AGENTS.md`. Verify: goldens pass, `--no-default-features` plus each single-effect feature builds, and the Mermaid check passes

## 6. Converter

- [x] 6.1 Scaffold `crates/synth-convert` (std; not in `default-members`; deps `synth-core`, `wavesabre`, `tfsynth`, `unisynth` with `std`). Add the `ws` command for song structure (header, tracks, receives, volumes, lanes, device order) and effect devices (mode + copied parameters, automation index + 1). Verify: the "WaveSabre song" (effects part), "Sidechain routing" and "Malformed input" scenarios, plus a round-trip test of every field the "Song structure preserved" requirement lists
- [x] 6.2 Implement the WaveSabre instrument mappings of design D10 (Falcon, Slaughter, Specimen, Adultery, Thunder), the capacity analysis with its warning, and automation re-mapping with point insertion. Run `doubt-driven-review` on the mapping rules. Verify: the "WaveSabre song", "Slaughter cutoff route", "Cutoff automation", "Filter type sweep" and "Thunder level" scenarios
- [x] 6.3 Implement Tunefish conversion: kind 32–39 devices in `synth-mix` songs, the `tfm` command (tracks, insert chains, block-start timing, master scaling), the `patch` command, the organ-gate rule, and rejection of automated slot kinds. Verify: the "Insert chain", "Tunefish organ gate" and "Loudness of a converted song" scenarios; block-start timing for tempo 125, row 1
- [x] 6.4 Implement the `features` command and a compo-build check of a converted song with exactly the listed features. Verify: the "Minimal features" scenario; the i686 build with the printed list succeeds
- [x] 6.5 Document the converter (commands, mapping table, automation rule) in `docs/synth/unisynth.md` and `crates/synth-convert/AGENTS.md`. Verify: the documented commands run as written on a synthetic song

## 6a. DAW support (requested by add-synth-daw)

- [x] 6a.1 Append the controller block (wheel 180, bend 181, bend range 182; `COUNT` 183; the sample indices own 177–179) with laws, names, defaults; route wheel to mod source 7 and `mod_wheel` to index 180; apply bend to every voice's pitch. Verify: the "Automated wheel" and "Bend range" scenarios; existing goldens and chunk tests unchanged; a chunk with index 182 decodes, 183 is rejected
- [x] 6a.2 Add the DLS-instrument sampler source (packed index, `Dls::find`, `region_for(key, vel)` at note-on, region/wave `wsmp` root key, fine tune and loop; articulation and `wsmp` gain ignored; feature `us-sample-dls`). Verify: the "Key split", "Drum kit" and "Instrument missing from the bank" scenarios against the real macOS bank (skipped if absent) and a synthetic two-region bank in the test
- [x] 6a.3 Document both in `docs/synth/unisynth.md` (layout table, sampler section) and `crates/unisynth/AGENTS.md`. Verify: `kb_search "unisynth bend"` and `"DLS instrument"` return the rows

## 7. Parity, speed, size (integration)

- [x] 7.1 Add `tools/unisynth-parity` (Bun): device-level pipeline (`wavesabre` `render_device` songs and `tfsynth` synthetic patches → legacy render, convert, unified render, `synth-parity` compare), the report writer (`report.md`: metrics only) and the skip path. Verify: `bun test tools/unisynth-parity` covers the report format and the skip scenario ("Songs absent")
- [x] 7.2 Run device-level parity for every legacy device and mode, and fix mismatches with `systematic-debugging` until all pass. Verify: the "Falcon test song" and "Cathedral mode parity" scenarios; the committed report shows every device passing
- [x] 7.3 Run reference-song parity (Global Groove, Elysian, Engage, Eladamri, no_remorse, tf4testsong) from the synth lab. Verify: the "Elysian" scenario; the report shows all six passing
- [x] 7.4 Measure speed (release, one thread) and size: add an i686 size probe like `research/ws-size-probe` for an all-modules build and each reference song's feature set, and record both in the report and `research/softsynth-research.md` next to the legacy figures. Verify: the "Size report" scenario; Elysian ≥ 5× and no_remorse ≥ 10× real time (optimise with `performance-optimization` if not)
- [x] 7.5 Update `docs/synth/README.md` (engine table and layers) and the audio section of `docs/architecture.md` for the unified synth. Run the retirement-gate check: the report marks retirement ready or blocked, naming any failing item. Verify: the "Failing song blocks retirement" scenario on a forced failure; docs render
- [x] 7.6 A/B listen: render the six reference songs both ways for the user and record their sign-off or findings in the report. Verify: the report holds the user's decision

## Workflow follow-up

- Archive this change after the review gates (`code-review`, `code-quality`) pass.
- Open `retire-legacy-synths` only if the report says ready and the user signed off (7.6). It deletes `crates/wavesabre`, `crates/tfsynth`, `tools/ws-parity` and `tools/tf-parity`, moves the `.tfm` decoder into `synth-convert` if still wanted, and updates the `synth-mix` kind registry requirement.
- Retarget `add-audio-to-synth` (`engines.toml`) to `unisynth`.
- [x] 7.7 Open filters are passed through (cutoff at the end of its range, no resonance): `flt::transparent`, `Slot::pass` with state priming on the way out; measured loss per filter, 1–6 % less render time on the WaveSabre songs, Global Groove listed as an accepted difference (±2.5 dB / ±4 dB) and the sampler tests allowed 4 dB in the top band. Verify: `an_open_filter_*`, `a_filter_that_wakes_*`, `a_passed_through_slot_*`, `a_low_pass_at_the_top_*`, parity 49/49.
- [x] 7.8 `unisynth` has no reference to the original engines: the comparisons with them (every effect mode, wavetable tables, ladder/RBJ/notch filters, noise band) moved to `synth-convert/tests/{effects,engines}.rs`, the dev-dependencies are gone, "legacy"/"upstream" wording replaced, the MIT notice kept as `crates/unisynth/LICENSE-WaveSabre`. Verify: `cargo tree -p unisynth -e dev` lists neither crate; `cargo test -p synth-convert` 17 + 4 comparison tests pass.
- [x] 7.9 Modes are named for their function: envelope `timed`/`rate`; distortion `shaper`/`power`; EQ `parametric`/`3band`; delay `tempo`/`ms`; flanger `phaser`/`ring`; reverb `freeverb`/`compact`/`rooms`. Features, module ids, source files, structs, messages, the DAW mode list and the docs follow; chunk values and goldens are unchanged. Verify: `check-unisynth-features.sh`, `cargo test -p unisynth -p synth-convert -p song`.
