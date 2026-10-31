# Proposal

## Why

The TBL XMs fake stereo and reverb with extra channels. A pad plays the same note hard left and hard right (one side
detuned by `E11`) and then repeats it once or twice on other channels, quieter and panned in turn (an echo). The doubled
hi-hat in Jizz plays one sample hard left and hard right. A pre-rendered phrase played at another pitch also gets a
track of its own (one track per playback-ratio class). The converted songs therefore have 58–106 phrase tracks:
Jizz record 0 alone has four, and Stash's `Jace41` about fifteen. The user wants the converted music compact. The
essence of the sound must stay, but the output need not match the original sample for sample.

## What Changes

- **Layers dropped (default).** A trigger that repeats a sounding trigger (same instrument, sample, note and offset, on
  another channel) is dropped. That covers one at the same tick and one that starts later while the first still sounds
  and is quieter (an echo tap). The kept trigger takes the dropped same-tick copies' level, centred (equal-power sum),
  so the pair keeps its loudness. Echo taps leave no trace: the result is dry.
- **Pitch tracks merged (default).** Phrase triggers of one record share tracks whatever their playback ratio. A
  trigger joins the first track of its record that it overlaps nowhere and whose pitch range it fits within one
  track's reach. The track class is the most-played class, moved inside that range. Stored-sample triggers pack the
  same way, ignoring the channel.
- **Options keep the old ways.** `--keep-layers` and `--split-pitches`. With both, the output is byte-identical to the
  current default. The parity tool passes both.
- `stderr` reports `layers N dropped (P same-tick, E echoes)`.
- Measured in `tools/unisynth-parity/cauldron-compact.md`: tracks, song size, xz, render time, loudness and bands
  against the reference, for the three songs in both modes. Renders for listening go to
  `research/tbl-mini-cauldron/out/compact/`.

## Discipline Skills

`doubt-driven-review` (the default output changes; the parity path must stay byte-identical),
`code-simplification`.

## Capabilities

### Modified Capabilities

- `synth-convert`: mini-cauldron XM import (layer and pitch-track options), track grouping (merged across ratios),
  song parity (judged on the split, layered conversion).

## Impact

`crates/synth-convert` (`cauldron/import.rs`, new `cauldron/import/compact.rs`, `main.rs`),
`tools/unisynth-parity/cauldron.ts` (passes the two options), docs. The default `synth-convert xm` output changes. The
demo songs (`make_demos.py`) become compact on their next build.
