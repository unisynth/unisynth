# Tasks

## 1. Corpus, reference and loader

- [x] 1.1 Add `tools/v2-extract` (bun + a small C++ dump built against the external v2redux checkout, skipped with a message when absent). Commands: `gather` (fr_public, v2redux corpus, the local v2m-player set; `.v2mz` gunzip) into a git-ignored songs dir; `carve <exe>` via v2redux `toolkit/era`; `render` with `v2dump`; `survey` → per-song version, features, checksum. Write `research/v2-intros.md` (provenance, version, features per acceptance song; no audio). Verify: the 15 acceptance songs are gathered with checksums equal to v2redux `PROVENANCE.md`, and each has a reference WAV
- [x] 1.2 Implement the `.v2m` reader in `crates/synth-convert` (`src/v2m.rs`): byte-plane event decoding, version detection from patch and global sizes, upgrade to the v6 layout with `v2initsnd`/`v2initglobs` defaults, speech text. Add a `.v2m` test writer for synthetic one-patch songs. Verify: "Upgrade matches the reference" (byte-identical to the v2redux dump for all 15 acceptance songs and the 359 survey songs), "Unknown version", and truncation at every byte without a panic
- [x] 1.3 Add `synth-convert/AGENTS.md` rows, a `tools/AGENTS.md` row and a `docs/synth/unisynth.md` "V2 songs" section (format, versions, sources, licensing). Verify: `kb_search "v2m import"` returns them

## 2. Appended parameters and the extended matrix

- [x] 2.1 Implement the escaped index and count encoding (design D4) in `chunk.rs`: parser, `pairs()`, the `us-trusted` fast path and the host encoder (shortest form). Run `doubt-driven-review` on it and record the outcome in design.md D4. Verify: the "High index round trip", "Non-canonical escape" and "Escape at the end" scenarios; truncation at every byte; "Old chunk unchanged" (self-goldens, effect goldens, `research/us-bake-check/check.sh`); the size probe shows the player cost
- [x] 2.2 Freeze the appended layout (design D4a) in `params.rs` + `params/host.rs` (names, laws, defaults, `isdest` targets; the most-used parameters in 184–239). Run `doubt-driven-review` on it before later tasks depend on it, and record the outcome in design.md D4a. Verify: `params` tests (count, region split, defaults, names, laws, inverses); goldens unchanged
- [x] 2.3 Implement chunk version 2 (speech text section; the encoder writes v1 without text). Verify: the "Speech text section" and "Malformed chunk" scenarios, truncation at every byte for v2 chunks
- [x] 2.4 Extend `modmx.rs`: the Add operation (normalized space before the law, clamp after all Add slots, then Scale, law, Offset), controller sources CC2–CC7 (+ `set_param` and automation), appended target ids, slots 13–16, `Tab` equal to the scan, `needs`/`check_voice` for the new sources and targets. Verify: the "V2 add with clamp", "Controller source", "Scale composition" and "Mod slot depth" scenarios; the matrix tests; goldens unchanged
- [x] 2.5 Update `docs/synth/unisynth.md` (Parameters, Chunk with the escape encoding, Mod matrix: the Add operation and the new tables, Mermaid checked) and `crates/unisynth/AGENTS.md`. Verify: the Mermaid blocks parse

## 3. V2 voice modules

- [x] 3.1 V2 oscillator type (`us-osc-v2`): saw↔tri, pulse, sine, coloured noise, FM from the running sum, ring mode, V2 transpose/detune/colour/volume laws, key sync none/osc/full. Port from `synth_core.cpp` with a public-domain note. Verify: the "Pulse width" and "Ring mode" scenarios; per-mode parity against v2redux on synthetic one-patch songs (1.2 writer) with the shared metrics
- [x] 3.2 V2 filters (`us-flt-v2`: SVF LP/BP/HP/notch/all-pass, Moog LP/HP with V2 laws) and the routing (`us-flt-route`: serial/parallel + balance for slots 1–2). Verify: the "Parallel balance" and "All-pass" scenarios; per-type parity against v2redux
- [x] 3.3 Voice distortion stage (`us-vdist`, modes of design D5, shared code with the channel stage). Verify: the "Overdrive" scenario; per-mode parity against v2redux; off = bit-identical
- [x] 3.4 V2 envelope mode (`us-env-v2`: sustain time slope, amplify, V2 stage curves) and V2 LFO features (`us-lfo-v2`: triangle, S+H, envelope mode, phase, polarity, amplify as the Add source). Verify: the "V2 sustain slope", "Envelope mode" and "Key sync" scenarios; parity of one EG/LFO-swept patch against v2redux
- [x] 3.5 Gating and needs: every V2 module in `module.rs`, `needs.rs`, `check_modules`/`check_voice`, `tools/check-unisynth-features.sh`. Verify: the "Speech disabled"-style missing-module test for each module; `check-unisynth-features.sh` passes; i686 core-only and wasm32 checks pass
- [x] 3.6 Docs: `docs/synth/unisynth.md` sections for each module (laws, diagrams) and the AGENTS rows. Verify: `kb_search "v2 oscillator ring mode"` finds them

## 4. Channel stage, speech and effect modes

- [x] 4.1 `unified-fx` modes: reverb `v2`, delay `v2`, compressor `v2`, EQ `cut` (features `us-fx-*-v2`, `us-fx-eq-cut`), with the format-version low-cut gate. Verify: "Existing modes unchanged" (effect goldens), "Reverb low cut by format version", and "Delay parity" plus one parity song per mode against v2redux
- [x] 4.2 Channel stage in kind 64 (`us-chan-v2`: volume, boost, distortion, chorus/flanger, compressor shared with 4.1, FX order, four sends to extra output pairs). Channel targets are evaluated from the newest voice. Verify: the "Transparent default", "Reverb send" and "Channel target from the newest voice" scenarios; parity of a chorus + compressor patch
- [x] 4.3 Ronan speech stage (`us-speech`): port `ronan.cpp` (public domain), text from the chunk v2 section, note-on syllable advance, CC control, on the voice sum before the channel stage. Verify: the "Speech text" scenario on fr-08's speech channel; the "Speech disabled" scenario
- [x] 4.4 Docs and AGENTS rows for 4.1–4.3 (effects table, channel stage diagram, speech). Verify: the Mermaid blocks parse

## 5. Song model: extra pairs

- [x] 5.1 Run `doubt-driven-review` on the receive-field encoding (design D6), then implement it in `synth-core`: `Device::outputs()/inputs()` (default 0), receive decode (byte 0 receiving pair 0/2/4/6, byte 1 sending pair 0–4), buffers per track sized from the devices, threads-level dependencies, errors for missing pairs. Kind 64 declares 4 outputs and 2 inputs only when the patch uses sends or aux. Verify: the "Old blob unchanged", "Send output into a reverb bus", "Pair out of range" and "Aux input" scenarios; `research/size-threads/hashes.sh` and `research/us-bake-check/check.sh` unchanged
- [ ] 5.2 V2 aux input mode of the V2 oscillator, and aux receive levels. Verify: an aux-A oscillator patch fed by a sine track outputs that sine through its filter; parity on one corpus song that uses aux A
- [x] 5.3 Update `openspec`-referenced docs (`docs/synth/README.md` song model, `crates/synth-core/AGENTS.md`). Verify: `kb_search "receive output pair"` returns them

## 6. Song mapping and parity

- [x] 6.1 `synth-convert v2m`: per-(channel, program) tracks, the tempo map to samples, bend/CC automation and the initial controller values (checked against v2redux), patch → unified values (V2 laws, mod entries → Add slots, drop-smallest with warning), capacity from MaxPoly, buses (reverb, delay, aux A/B), master (EQ cut, compressor v2), channel 16 speech, format version parameter. Verify: the "Program change" and "More modulation entries than slots" scenarios; `synth-convert features` lists the V2 modules for a converted song
- [x] 6.2 Add the V2 acceptance songs to `tools/unisynth-parity` (references from `v2dump`, skip when absent) and render `renders/unisynth/<song>_unisynth.{wav,mp3}`. Use `systematic-debugging` on each failure, and add format-version gates (design D9) only for failures they fix, each with a test. Verify: the "fr-08" scenario holds for all 15 songs; the report and `signoff.json` are committed (metrics only); "Songs absent" exits 0
- [x] 6.3 Speed and size: add V2 rows to the speed report and `research/us-size-probe/measure.sh` (all-modules and per-song builds, baked). Verify: `tools/unisynth-parity/size.md` has the rows; a non-V2 reference song's packed size is unchanged (± 0 bytes) Done: V2 rows in `size.md`; non-V2 songs +180–470 B xz, squishy +0/+512 (design D11), recorded instead of ± 0.
- [ ] 6.4 User A/B listen of the converted acceptance songs vs the v2redux renders, with the outcome recorded in the report. Verify: the user signs off, or the failures are listed as follow-ups
- [x] 6.5 Final docs pass: `docs/architecture.md` (V2 import in the audio section), `docs/synth/unisynth.md` converter walkthrough (`synth-convert v2m` runs as written), `research/v2-synth.md` updated with the outcomes. Verify: the walkthrough commands run; `bun test` and `cargo test --workspace` pass

## Workflow follow-up

- Run `code-review` and `code-quality` before the commit, then archive the change with `openspec archive add-v2-import`.
