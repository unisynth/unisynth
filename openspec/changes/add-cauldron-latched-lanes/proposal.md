# Proposal

## Why

The converted TBL songs show dense per-note lanes in the DAW: each phrase step writes its retrigger, start-phase and
age bytes as a hold-and-jump pair, so a retrigger count rising by one per note is a staircase of thousands of points.
The original computes a step's filter at its retrigger and keeps it, so those bytes only matter at note-ons.

## What Changes

- **unisynth:** a cauldron slot latches its filter settings (base, amount, decay, damping) at a non-legato note-on
  and keeps them for the note (`CauldronSlot::flt`).
- **Converter:** the retrigger, phase and age lanes are written as latched lanes: each note's byte holds exactly
  from its step through `LATCH_WINDOW` (lane lead + 128 frames); between notes the lane is free, so bytes on a line
  share one ramp. Pitch lanes (fine, coarse, word) stay exact steps.
- **Demo songs** convert with `--thin 2`.

## Capabilities

### Modified Capabilities

- `unified-synth`: cauldron filter latched at note-on.
- `synth-convert`: per-note bytes as latched lanes.

## Impact

`crates/unisynth/src/osc.rs`, `crates/synth-convert/src/cauldron.rs`, `make_demos.py`, docs. Parity unchanged (85
of 88; Jizz +0.33, song 1 +0.23, BLUISH +0.13 dB). Jizz retrigger points 10 874 → 3 637; demo-song automation −29 %
(Jizz) with `--thin 2`. The default conversion is no longer byte-identical to the previous one.
