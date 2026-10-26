# docs/synth

Synth architecture docs (Mermaid diagrams). Source of truth = crates; update the doc when a crate's behaviour changes.

| File | Purpose |
|---|---|
| `daw.md` | DAW (change add-synth-daw): pieces diagram (song/1 → crates/song → blob → Player / synth-wasm Host → packages/daw worklet, worker, /api/music), formats (song/1 example, export/import/validation rules), live vs bounce table + measured load (task 3.7), how to start, shortcuts, not-yet list (ctl.wheel/bend 6a, synth-convert library loads, DLS-instrument source). |
| `README.md` | Index; layer diagram; shared rules (no_std, no heap, D2 sharing rule, kind features, determinism); WaveSabre vs Tunefish comparison table. |
| `synth-core.md` | `crates/synth-core`: `Device` contract + `Io` channel roles, `Ctx`, `Events<N>` split, `VoicePool` (unison/steal/mono legato flow), `dsp` primitives, song blob layout + ER diagram, `Kinds`/`Either`, `Player` load + per-sub-block flow, determinism (100-frame step), pool memory. |
| `tunefish4.md` | `crates/tfsynth`: instrument, per-block voice flow, pitch, wavetable generator (spectrum, phase stage, IFFT), unison, noise, filters, ADSR/LFO, mod matrix, 10-slot fx chain + effect table, 112-param map, `.tfm`/`TfmPlayer`, kind 32–39 adapter sequence, quirk policy. |
| `unisynth.md` | `crates/unisynth` + `crates/synth-convert` (add-unified-synth): where it sits, instrument slot diagram, 184-parameter layout (sample indices, controllers, noise width), fine window + inaudible releases, parity/speed/size report + per-type `shape`, laws table, chunk v1 format + validation flow, structural-parameter rule, voice framework (events → 32-frame control grid sequence, control segments, envelope modes, LFOs), mod matrix (evaluation, target table), oscillator slots (phase-mod operator, saw/pulse), effects (mode table), converter (`ws`: effect mapping table, in-place device mapping; WaveSabre instruments; Tunefish instrument, organ gate, `tfm` songs, kind 32 with insert effects; `features`, the compo-build check and a runnable walkthrough). Grows per task group. |
| `wavesabre.md` | `crates/wavesabre`: 13-device table (kinds 0–12), crate graph, load path, helper curves, `Synth`/`Envelope`, Falcon PM + Slaughter BLIT diagrams + param maps, sampler (GSM/DLS sources, SamplePlayer), every effect diagram, example song graph. |
