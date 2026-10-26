# Proposal

## Why

We now have two synth engines on one shared core (`crates/wavesabre`, `crates/tfsynth` on `crates/synth-core`). They
share the song format, the mixer, the voice pool and a few DSP primitives, but each sound still lives in one engine's
fixed instrument. A Tunefish wavetable can't go through a WaveSabre SVF, and a Slaughter oscillator can't use the mod
matrix. Linking both costs about 27.6 KB + 18.9 KB packed. We want one instrument that has the sound-making modules of
both engines, plus one set of effects, so that both engines can be retired. `add-wavesabre-rs` and `add-tf-synth`
deferred this as "option 3" (`research/softsynth-research.md`, Decision 2026-10-25).

User decisions (2026-10-26):
- The unified synth **replaces** both engines. Old patches and songs must sound **close** (perceptual parity through
  `tools/synth-parity`), not bit-identical.
- The voice has a **fixed set of slots** (no free node graph).
- **One control rate** for all modules: a short fixed control block with linear interpolation.
- **No hard size target**: sizes are measured and reported per build.
- A **host-only converter** turns old WaveSabre song blobs and `.tfm` files into unified songs. The effects merge into
  one set, and each different algorithm stays as a mode. The old crates retire only after the converted reference songs
  pass parity.

## What Changes

- New `no_std` f32 crate `crates/unisynth` (MIT; WaveSabre's MIT notice kept) on `synth-core`. No heap, no `core::fmt`.
  It plays through the existing `synth-mix` song player.
- **Unified instrument** (song kind 64), one fixed voice layout:
  - 3 oscillator slots. A slot's type is phase-mod operator (Falcon), band-limited saw/pulse (Slaughter), spectral
    wavetable (Tunefish, with its unison layers), or sampler (GSM 6.10 sample in the chunk, or GM.DLS wave).
    Phase modulation can go from a slot into the next one.
  - 1 noise source, with an optional band filter.
  - 4 serial filter slots: SVF LP/HP/BP/notch, ladder LP, RBJ HP/BP, or notch.
  - 4 envelopes (envelope 1 shapes amplitude and ends the voice), each with a WaveSabre or Tunefish curve mode.
  - 2 LFOs.
  - A 12-slot mod matrix. Each slot either scales the target's normalized value (product, like Tunefish) or adds an
    offset in the target's physical unit (sum, like WaveSabre's pitch and cutoff routes).
  - Voice settings: capacity, stealing policy, voice-level unison, mono legato with slide, glide, pitch-bend range.
  - Control runs on one 32-frame block with linear interpolation, and audio runs per sample. Notes start on their
    sample. Each device has its own random source, seeded at load, so output does not depend on the player's sub-block
    size.
  - One shared set of numerics: one sine, one random law, one dB law. There is no WaveSabre FastSin table or `2^(dB/6)`
    law in the instrument.
  - Each module type is a cargo feature, so a song links only what it uses.
- **Unified effects** (kinds 65–73): distortion, crusher, EQ, delay, chorus, flanger/phaser, reverb, compressor,
  formant. Where both engines had an algorithm for a job, each becomes a mode (for example reverb: freeverb, compact,
  rooms). A mode keeps its legacy parameter layout and curves, so effect parameters and automation convert unchanged.
  Effects are track devices; the instrument has no insert slots.
- **Converter** `crates/synth-convert` (std, host-only, never in the player). It turns a WaveSabre song blob or a `.tfm`
  into a unified `synth-mix` blob:
  - instruments become slot configurations;
  - instrument automation is re-mapped through the new parameter curves;
  - a Tunefish instrument's insert chain becomes track devices after it.

  It can also print the cargo features a song needs.
- **Parity gates.** Converted songs are rendered by `unisynth` and compared with the legacy crates' renders of the
  originals. Two sets are compared: every legacy device's synthetic test song, and the reference songs (WaveSabre
  Global Groove, Elysian, Engage and Eladamri; Tunefish no_remorse and tf4testsong). A report gives the metrics, render
  speed and packed size per build.
- Extract the GSM 6.10 decoder from `crates/wavesabre` into its own `crates/gsm`, used by both crates. WaveSabre's
  behaviour does not change.
- The legacy crates stay as frozen parity oracles. Deleting them is a separate later change (`retire-legacy-synths`),
  gated on this change's parity report and the user's A/B sign-off.

Out of scope: deleting `crates/wavesabre`/`crates/tfsynth` and their parity tools (`retire-legacy-synths`). Also out of
scope: player audio output and timeline sync, the editor and wasm UI, authoring tools and a plugin UI, a free node-graph
instrument, and 64klang.

## Discipline Skills

`doubt-driven-review` (the instrument chunk layout, parameter map and conversion rules are format contracts that are
hard to change later), `performance-optimization` (prerender speed, control-block cost, per-voice wavetable rebuilds),
`systematic-debugging` (parity mismatches between converted and legacy renders), `security-hardening` (the chunk
decoder and the converter parse untrusted song bytes), `code-simplification` (merging two engines' code paths into
one).

## Capabilities

### New Capabilities
- `unified-synth`: the unified instrument. Covers the device kind and chunk, the slot topology and module types, the mod
  matrix, control rate, voice allocation, samplers, determinism, module features, and speed and size reporting.
- `unified-fx`: the merged effect set. Covers the kinds and modes, the per-mode legacy parameter layouts, buffer roles
  and features.
- `synth-convert`: converting legacy songs and patches into unified songs. Covers instrument and effect mapping,
  automation re-mapping, the features list, parity gates against the legacy engines, and the retirement gate.

### Modified Capabilities
- (none) Kind ids 64–73 fall in the "other engines from 32 upward" range of `synth-mix`. The GSM extraction keeps
  `ws-synth` behaviour unchanged.

## Impact

- New crates `crates/unisynth` (in `default-members`) and `crates/synth-convert` (std, host-only, not in
  `default-members`). New crate `crates/gsm`; `crates/wavesabre` depends on it instead of its own `gsm.rs`.
- `crates/wavesabre` and `crates/tfsynth` are frozen apart from the GSM extraction. During the transition, mixed songs
  can load all three registries through `synth_core::mix::Either`.
- New `tools/unisynth-parity/` (Bun). It drives the converter, renders legacy vs unified, and reuses
  `tools/synth-parity` metrics. It runs opt-in for the reference songs, which stay uncommitted in the synth lab.
- Docs: `docs/synth/unisynth.md` (new), `docs/synth/README.md`, `docs/architecture.md` audio section, `crates/AGENTS.md`,
  per-crate `AGENTS.md`, and the measured results in `research/softsynth-research.md`.
- `add-audio-to-synth` (in progress, 0/27) targets both legacy engines through `engines.toml`. Once this lands, that
  change should target `unisynth` only. That is noted here; this change does not edit it.
- Licensing: everything stays MIT. WaveSabre-derived code carries its notice. Tunefish-derived code is our own MIT
  implementation from `crates/tfsynth`.
