# Proposal

## Why

A converted TBL pad track carries a long effect chain: up to three clips, the cauldron echo, and a two-stage
band-limiting low-pass (two tempo delays used as SVF high cuts), before the post gain, stereo delay and reverb. In the
DAW this reads as the same filters stacked several times. The user wants the chain slimmed for the compact layout:
one low-pass stage, no low-pass where its cut is inaudible, and no clips that never act.

## What Changes

- **One low-pass stage (default).** The band-limiting low-pass keeps one SVF high-cut stage (12 dB/octave) instead
  of two. Its frame of latency goes with the dropped stage.
- **No low-pass from 16 kHz (default).** A track that does not slide gets no low-pass when its cut (stored Nyquist
  × playing ratio) is at least 16 kHz. A sliding track keeps its stage, because its cut follows the slide down.
- **Only working clips (default).** Each clip (shaper clipper) of a phrase track is checked with a solo render of the
  track's devices before it, playing the track's own notes and automation. A clip whose input never passes its
  threshold (by more than −80 dB) is a no-op and is left out, so this step does not change the sound. The checks run
  per track in parallel. The measured result: most clips work. The cauldron's 16-bit clamp right after the
  instrument is hit by up to +37 dB, and the echo clip by up to +6 dB. Only Stash song 1 (2) and BLUISH (25) have idle
  clips.
- **`--exact-chain`** keeps the original chain (two stages up to 20 kHz, every clip). With
  `--keep-layers --split-pitches --exact-chain`, the output is byte-identical to the conversion before this change. The
  parity tool passes all three.
- `stderr` reports `clips N idle left out`.

## Discipline Skills

`doubt-driven-review` (the default output changes; the parity path must stay byte-identical),
`performance-optimization` (the clip check's solo renders add conversion time).

## Capabilities

### Modified Capabilities

- `synth-convert`: compact layout options (`--exact-chain`), song parity options; new requirement for the slim chain.

## Impact

`crates/synth-convert` (`cauldron.rs` `Chain::lp_stages`, `cauldron/import.rs`, `cauldron/import/compact.rs`,
`main.rs`), `tools/unisynth-parity/cauldron.ts` and `cauldron-compact.ts`, docs. The demo songs get the slim chain on
their next build.
