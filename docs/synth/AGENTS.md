# docs/synth

Synth architecture docs (Mermaid diagrams). Source of truth = crates; update the doc when a crate's behaviour changes. Long rows have pull-only sidecars `<doc>.AGENTS.md`.

| File | Purpose |
|---|---|
| `architecture.md` | Audio architecture overview moved from 64k `docs/architecture.md`: synth-core/synth-mix layers + engines, `tfsynth`, `unisynth` + converter, DAW (song source, live host, browser editor), multi-threaded rendering (add-synth-threads, add-wasm-threads `synth-mt.wasm` bounce). |
| `daw.md` | DAW (add-synth-daw): pieces diagram (`song/2` → `crates/song` → blob → `Player` / `synth-wasm` Host → `packages/daw`), formats (export/import/validation), live vs bounce + load, GSM samples, device panel live view, UI structure, using the DAW, keys. → see `daw.md.AGENTS.md`. |
| `glorp.md` | `crates/glorp` on the player: kind table (all fourteen ids incl. 30), why a preflight gate, what is refused, automation boxes, fault latch, verification, whole song through the player (60.397 % exact, worst 136 steps), ID 21 + `Device::lane_lerp`, speed/size summary (external probes), not claimed. |
| `README.md` | Index + layer diagram; shared rules (no_std, no heap, D2 sharing rule, kind features, determinism); legacy engine comparison table; unified synth next to them (V2 import, kinds 64–73). |
| `synth-core.md` | `crates/synth-core`: module map, `Device` contract + channels/`Ctx`/`Events<N>`, `VoicePool`, `dsp` primitives, song blob layout, `Kinds`/`Either` registry (`preflight`, `block` hooks, per-device `lane_lerp`), `Player` load + per-sub-block flow, quiet tracks sleep (`quiet_skip`), determinism (100-frame step), pool memory. |
| `tunefish4.md` | `crates/tfsynth`: crate structure, instrument, per-block voice flow, pitch, wavetable generator, unison, noise, filters, ADSR/LFO, mod matrix, 10-slot fx chain, 112-param map, `.tfm`/`TfmPlayer`, kind 32–39 adapter, `.tf4m` (2013 engine), quirk policy, verification. |
| `unisynth.md` | `crates/unisynth` + `crates/synth-convert`: instrument slots, v3 layout (294 params) + laws, chunk v3, voice framework, mod matrix, oscillator slots, noise/filters, V2 modules, effects kinds 65–73, converter (WaveSabre, Tunefish, V2, mini-cauldron XM), parity/speed/size. → see `unisynth.md.AGENTS.md`. |
| `wavesabre.md` | `crates/wavesabre`: 13-device table (kinds 0–12), crate structure, load path, helper curves, `Synth`/`Envelope`, Falcon PM + Slaughter BLIT, sample instruments (GSM/DLS sources, sampler), every effect, example song graph, verification. |
| (external) gm-procedural | Procedural GM bank docs live at https://github.com/unisynth/gm-procedural (`docs/synth/gm-procedural.md`); listening pages https://storage.googleapis.com/gm-procedural/index.html. |

