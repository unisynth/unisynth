# openspec

Spec-driven changes. Active change = `changes/<name>/` (`proposal.md` why/what, `design.md` decisions D1…, `tasks.md`,
`specs/<capability>/spec.md` deltas). Archived = `changes/archive/<date>-<name>/`. Living specs = `specs/<capability>/spec.md`.
Each folder is self-describing: read its `proposal.md` / spec `## Purpose` first. Scaffold with `openspec change new <name>`.

| Path | Purpose |
|---|---|
| `config.yaml` | OpenSpec config: `schema: spec-driven`, no project context / rules yet. |
| `changes/add-audio-to-synth/` | Active. mp3 → stems (bass, lead, drums) → notes + fitted unified-synth patches that sound close to the original. Capabilities `audio-transcribe`, `synth-patch-fit`, `synth-render`. |
| `changes/add-import-patterns/` | Active. `song import` cuts each blob lane into named, repeating patterns (not one song-long clip `Track N`). Delta on `song-source`. |
| `changes/archive/2026-10-25-add-wavesabre-rs/` | no_std Rust WaveSabre port on a shared core. Created `synth-core`, `synth-mix`, `ws-synth`. |
| `changes/archive/2026-10-26-add-synth-threads/` | Opt-in multi-core native render, bit-identical to single-thread. `synth-threads`. |
| `changes/archive/2026-10-26-add-tf-synth/` | MIT no_std Tunefish-4-compatible engine + `.tfm` player. `tf-synth`, `tf-song`. |
| `changes/archive/2026-10-26-add-unified-synth/` | One slot-based instrument (kind 64) + merged effects (65–73) replacing both legacy engines; converter. `unified-synth`, `unified-fx`, `synth-convert`. |
| `changes/archive/2026-10-27-shrink-unified-synth/` | Cut packed size of unified-synth builds back toward the legacy engines (squishy, compo config). |
| `changes/archive/2026-10-28-add-wasm-threads/` | Threaded wasm engine for DAW offline bounce when cross-origin isolated. `synth-wasm-threads`. |
| `changes/archive/2026-10-29-add-v2-import/` | Farbrausch V2 `.v2m` import (v2redux reference) into the unified synth. |
| `changes/archive/2026-10-30-add-cauldron-xm-import/` | TBL Jizz / Stash XMs with mini-cauldron synthesized samples → unified songs (cauldron oscillator). |
| `changes/archive/2026-10-30-add-device-live-view/` | DAW Unisynth device panel: stable slot layout + live values while notes play. |
| `changes/archive/2026-10-30-add-synth-daw/` | Browser DAW (arrange, notes, patches, mix) on `song/1` + live wasm host. `synth-daw`, `synth-live-host`, `song-source`. |
| `changes/archive/2026-10-31-add-cauldron-control-rate/` | XM import option: volume / pan / pitch per tick (default), row or note. |
| `changes/archive/2026-10-31-add-cauldron-lane-thinning/` | XM import `--thin`: slides / fades as few ramps within a tolerance. |
| `changes/archive/2026-10-31-add-cauldron-latched-lanes/` | Per-note retrigger / phase / age bytes as latched lanes, not staircases. |
| `changes/archive/2026-10-31-add-quiet-track-skip/` | Mixer skips silent tracks' devices (sparse tracker songs render faster than real time). |
| `changes/archive/2026-10-31-add-tbl-demo-songs/` | The three TBL soundtracks import and open in the DAW like released songs. |
| `changes/archive/2026-10-31-compact-cauldron-xm-import/` | Drop fake-stereo / echo XM channels, restore them as stereo delay + reverb. |
| `changes/archive/2026-10-31-slim-cauldron-chain/` | Shorter converted pad effect chain (one low-pass stage, only reached clips). |
| `changes/archive/2026-11-02-unify-v2-algorithms/` | Fold V2-specific modules into shared unisynth algorithms; v3 layout, `song/2`, migrate. |
| `changes/archive/2026-11-03-daw-solid-ts/` | Rewrite DAW UI from one untyped `app.js` closure to typed Solid + TS components. |
| `specs/song-source/` | `song/1` source + `us-patch/1` presets ↔ `synth-mix` blob. |
| `specs/synth-convert/` | Legacy WaveSabre / `.tfm` / Tunefish patch → unified songs, parity checks, legacy-removal gate. |
| `specs/synth-core/` | Shared no_std DSP primitives + device / voice / pool framework. |
| `specs/synth-daw/` | Browser DAW behaviour (largest spec). |
| `specs/synth-live-host/` | Wasm engine playing `song/1` live while edited, identical to the player when unedited. |
| `specs/synth-mix/` | One song + mixer model for all engines; WaveSabre-based binary format. |
| `specs/synth-threads/` | Opt-in native multi-threaded render, bit-identical, zero cost when off. |
| `specs/synth-wasm-threads/` | Threaded browser render when cross-origin isolated, identical output. |
| `specs/tf-song/` | Tunefish 4 `.tfm` song playback + verification vs original. |
| `specs/tf-synth/` | MIT no_std Tunefish-4-compatible instrument, no heap, deterministic. |
| `specs/unified-fx/` | One effect set, legacy algorithms kept as modes of each kind. |
| `specs/unified-synth/` | Fixed-slot instrument holding both legacy engines' modules. |
| `specs/ws-synth/` | MIT no_std WaveSabre device port, same sound. |
