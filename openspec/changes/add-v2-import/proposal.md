# Proposal

## Why

Farbrausch's V2 synth carries a large body of demoscene music: fr-08, flybye, candytron, .kkrieger, debris, and about
350 more songs on modland and in the v2m-player collection. V2's code is public domain, and `spheenik/v2redux` gives a
CC0 reference player that plays every format version (0–6) the way its own era's engine did. A survey of 359 songs
(`research/v2-synth.md`) shows that nearly every V2 module is in wide use. `unisynth` has no equivalent for ring mod,
V2 FM, parallel filters, all-pass/Moog filters, per-voice distortion, sustain-slope envelopes, one-shot and
sample-and-hold LFOs, V2's additive mod matrix, the per-channel strip, V2 reverb and delay, aux buses, or Ronan speech.
Importing V2 songs therefore needs those functions in `unisynth`, kept as their own modes, the same way the WaveSabre
and Tunefish algorithms were kept.

User decisions (2026-10-28):
- "v1/v2/v3" means the `.v2m` **format versions 0–6**. All of them load, and old ones upgrade the way `conv2m` does.
- **Fidelity**: the parity metrics already in use (1 dB loudness, 3 dB per octave band, onsets), not bit-exact.
- **Mod matrix**: extend the existing matrix so it can carry V2 routings, instead of adding a second matrix.
- **Parameters**: new parameters are **appended after the last index** (183), as noise width was. Indices of 240 and above use a bank-prefix escape byte (like UTF-8), so existing chunks keep their bytes.
- **Acceptance songs**: the 13 demo songs carved by v2redux (fr-08, flybye, fr-014, fr-019, fr-022 ×2, fr-024,
  fr-027, fr-029, candytron, brüllwürfel, farbomat, .kkrieger) plus pzero and v2_zeitmaschine.
- **Ronan speech is in scope.**

## What Changes

- **V2 song import**: `synth-convert v2m <in.v2m> <out.song>`. It detects the format version, upgrades the file to the
  v6 layout, and maps the 16 MIDI channels, program changes, CC 1–7, pitch bend and the tempo map onto `synth-mix`
  tracks, lanes and automation. Global effects go on bus and master tracks.
- **Song corpus and reference renders** (local, git-ignored): `tools/v2-extract` collects songs from fr_public, v2redux
  and modland, carves demo `.exe` files with the v2redux toolkit, and renders references with `v2dump`. Provenance,
  version and features are recorded in `research/v2-intros.md`.
- **Unified instrument (kind 64), new modes and modules**, each behind its own `us-*` feature:
  - V2 oscillator slot type (saw↔triangle, pulse, sine, coloured noise, V2 FM from the previous slot, ring mod, aux
    bus input).
  - V2 filter types (SVF LP/BP/HP/notch/all-pass with the V2 law, Moog LP/HP) and a routing mode for filter slots 1–2:
    serial or parallel with balance.
  - Per-voice distortion stage (overdrive, clip, bit crush, decimate, filter modes).
  - V2 envelope mode (sustain time slope, V2 curves, amplify).
  - V2 LFO features (triangle, sample-and-hold, envelope/one-shot mode, polarity, start phase, key sync).
  - **Extended mod matrix**: a third operation `Add` (amount × source added to the target's normalized value before its
    law, clamped to 0..1, which is V2's rule), V2 controller sources (breath, CC 3–6, volume), and targets for every
    new parameter, including the channel stage. The slot count goes from 12 to 16.
  - **Channel stage** after the voice sum: volume, bass boost, channel distortion, chorus/flanger, channel compressor,
    effect order, and sends to reverb, delay and aux A/B. The matrix can modulate it from the newest voice, as V2 does.
  - **Ronan speech stage** on the voice sum: phoneme text in the chunk, advanced by note-ons, controlled by CCs.
- **Effects (kinds 65–73)**: new modes V2 reverb, V2 modulated stereo delay, V2 compressor (sum compressor; the
  channel stage uses the same code), and low/high cut.
- **Song model**: a device can send extra stereo output pairs (the instrument's send outputs) and receive extra input
  pairs (aux A/B oscillator input). This is encoded so that existing song blobs decode unchanged.
- **Parity**: `tools/unisynth-parity` gains the V2 acceptance songs, compared against v2redux renders with the existing
  metrics.

Out of scope: bit-exact output, V2's VST GUI, era-specific behaviour of format version 2 (no file of that version is
known), and committing any song audio.

## Discipline Skills

`doubt-driven-review` (the parameter-block layout and the song-model encoding are frozen once songs exist),
`systematic-debugging` (parity gaps), `performance-optimization` (the speed report and the per-feature size cost).

## Capabilities

### New Capabilities
None. V2 support extends existing capabilities.

### Modified Capabilities
- `unified-synth`: new oscillator type, filter types and routing, voice distortion, envelope and LFO modes, the
  extended mod matrix (Add operation, sources, targets, 16 slots), channel stage, speech stage, appended parameters.
- `unified-fx`: new effect modes (V2 reverb, V2 delay, V2 compressor, low/high cut).
- `synth-mix`: extra output and input pairs on devices, carried by receives.
- `synth-convert`: `.v2m` import (versions 0–6), mapping rules, V2 reference song parity.

## Impact

- Code: `crates/unisynth` (new modules, params, matrix, chunk), `crates/synth-core` (mixer receives with output and
  input pairs), `crates/synth-convert` (`v2m` reader and mapping), `tools/unisynth-parity`, new `tools/v2-extract`.
- Docs: `docs/synth/unisynth.md`, `docs/architecture.md`, the directory `AGENTS.md` files, `research/v2-intros.md`.
- Dependencies: none at build time. v2redux is used only as an external reference renderer, outside the repo.
- Licensing: V2 is public domain and v2redux is CC0, so code may be ported. Songs are © their authors and stay local.
- Size: a song that uses no V2 module builds without any of it (feature gating). V2 songs pay for what they use. The
  size report gains V2 rows.
