# Tasks

## 1. Fixed slot blocks (synth-daw)

- [x] 1.1 Rewrite test 5.6a in `packages/daw/test/editing.test.ts` for clear-in-place and filter swap (positions stable, mod targets kept, cleared slot's mod rows and lanes removed, one undo); add a test that setting filter 3's type in its collapsed header expands it at position 3; verify both fail
- [x] 1.2 Replace `slotRack` with fixed blocks: every slot rendered at its index, collapsed iff not set, type control in the collapsed header, ✕ = clear via `remapSlots(i → 0)`, ⇅ between neighbouring filters, no "+ slot" and no ↑/↓ for other groups; fixed column widths
- [x] 1.3 Mod matrix as a 12-row table (source, target, operation, amount) with dimmed None rows; env/LFO routing hints and dimmed knobs when unrouted
- [x] 1.4 Collapse/expand on slots (session override per track) and blocks (`store` key `daw.blk.<name>`), `aria-expanded`; test that a collapsed block survives a track change and the song is unchanged

## 2. Colours and static screens (synth-daw)

- [x] 2.1 Block accent tokens (`--acc` per `data-k`) in `style.css`: header strip, tint, knob arcs, mod-row source mark; unset slots neutral; axe test stays clean
- [x] 2.2 Static screens: oscillator waveform per type, filter response (log axis, cutoff mark) per type, LFO cycle; hook into `redrawGroup`; test that turning filter 1 cutoff moves the response curve's cutoff mark

## 3. Voice probe in unisynth (unified-synth)

- [x] 3.1 Doubt-driven review of the probe hook points in the voice loop (`voice.rs` control block, filter chain, sampler state) before code
- [x] 3.2 Tests first: probe reports envelope 1 sustain stage at level 0.5 for a held note; voice ids differ across a steal; LFO phase advances by rate × time; sampler position advances; goldens unchanged with `probe` on and taps enabled
- [x] 3.3 Feature `probe = ["std"]`: `Probe` snapshot (per voice id, note, amp, released, ages, env levels/stages, LFO phases, sampler positions, cutoff Hz) and `UInstrument::probe`
- [x] 3.4 Filter taps: `set_taps`, per-slot 2048-frame in/out ring summed over voices, flag hoisted per control block
- [x] 3.5 Confirm the player and size probes do not enable `probe` and the i686 size is unchanged (`measure-unisynth-size-change` procedure) — same-tree A/B of the i686 probe (compo config): showcase/us-all raw 119 296 → 119 296, xz 57 988 → 57 968–57 972; no-remorse raw 107 008 → 107 008, xz 29 064 → 29 052; re-measured with the final (cfg-gated) form after other changes landed in the tree: showcase raw 126 976 → 126 976, xz 62 972 → 62 908

## 4. Live host and worklet (synth-live-host)

- [x] 4.1 `synth-wasm`: build unisynth with `probe`; `probe_watch(track)`, `probe_ptr`/`probe_len` flat layout; Rust test: watched vs unwatched render bit-identical and equal to `song render`
- [x] 4.2 Worklet: append the probe slice (transferable) to `status` only while watching; `engine.ts` `watch(trackIndex)`; browser test that a chord on the watched track yields one voice record per sounding voice
- [x] 4.3 Measure with `bench/dense.ts`: unwatched render time unchanged within noise; watched with taps ≤ 5 % extra; record numbers in `docs/synth/daw.md` — watched: +1.5–3 % (dense track alone), +0.5 % (12 tracks) ✓; unwatched: +4.5–5 % vs the build without `probe` (0.485 → 0.51 ms/128 frames, 5.56 → 5.85 ms) ✗ — the tap branch in the voice loop; a separate tapped loop measured worse (+11 %, it changes inlining). Accepted (user): spec budget is now ≤ 5 % unwatched, and a Live toggle in the instrument header turns the view off

## 5. Live displays (synth-daw)

- [x] 5.1 Watch the selected track while the Device tab is visible; voice colour assignment by id (16-entry palette), note chips with "+N" beyond 16 (loudest first)
- [x] 5.2 Overlay loop (rAF, overlay groups only): sampler playheads, envelope dots (age-based x via `envCurve` geometry) and level bars, LFO dots, bead stacking, amplitude-driven brightness, faded released voices
- [x] 5.3 Filter spectra: 2048-point Hann FFT of each tap, log binning, peak hold; input grey, output `--acc`, per-voice cutoff ticks
- [x] 5.4 Browser tests: four-note chord on a sampler → four chips, four playheads, four dots per envelope; silence → no markers; low-pass at 300 Hz → output below input above the cutoff
- [x] 5.5 Measure overlay + FFT per frame at 16 voices and 4 filters (≤ 2 ms); fix before closing if over — 16 voices, 4 filters: probe parse + 8 FFTs 0.50 ms per status, overlay 0.49 ms per frame (`DAW.debug.live()`)

## 6. Docs and close-out

- [x] 6.1 `docs/synth/daw.md` (device panel, live view, probe data layout), `docs/synth/unisynth.md` (probe feature), directory `AGENTS.md` rows for changed files
- [x] 6.2 Full gates: `bun test` (daw), `cargo test --workspace`, `openspec validate add-device-live-view --strict`, code review — daw 33/33, unisynth (+`probe`) and synth-wasm green, validate ok, review fixes applied (colour reuse, probe layout guards, parallel-route Off taps, ring length assert, init order). `cargo test --workspace` has 3 failures outside this change (unisynth `fx::golden` delay mode 2, synth-convert filter-type decode ×2: files edited by other in-progress work)

## Workflow follow-up

- Archive after `add-synth-daw` (this change modifies requirements that change introduces).
