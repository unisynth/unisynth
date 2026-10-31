# Proposal

## Why

A converted XM writes its volume, pan and pitch as per-tick steps, two automation points per change (the player
interpolates linearly). A slide or fade of small steps can be drawn with far fewer points if the lane may differ a
little from the steps.

## What Changes

- `synth-convert xm … --thin <bytes>` (default 0: unchanged): the volume, pan, bend and low-pass lanes may differ from
  the per-tick step function by up to that many bytes (of 255) at any frame. Small steps become ramps through as few
  points as fit; larger steps, and the bend's step where the transpose changes, stay exact. Transpose, restart and
  start lanes are never thinned.
- `Options { controls, thin }` and `convert_with` replace `convert_at`.
- Measured (Stash song 1): raw song −10 / −18 / −30 % at 1 / 2 / 4 bytes, xz −2 / −5 / −8 %; sound within 0.15 dB
  loudness and 0.1 dB per octave band of `tick`.

## Capabilities

### Modified Capabilities

- `synth-convert`: mini-cauldron XM import (thinning option).

## Impact

`crates/synth-convert` (`cauldron.rs`, `cauldron/import.rs`, `main.rs`), `docs/synth/unisynth.md`. Default output
byte-identical.
