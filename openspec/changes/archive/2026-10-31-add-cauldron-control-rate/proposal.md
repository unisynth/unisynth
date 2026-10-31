# Proposal

## Why

The converted TBL songs carry the XM's volume, pan and pitch at every tracker tick: most of their size (Stash song 1:
245 000 automation points). A shorter version should be one option away, with the current output kept as the default.

## What Changes

- `synth-convert xm … --controls tick|row|note`: the XM control reaches the song every tick (default, unchanged), at
  row starts (slides and fades become steps; tremor and vibrato go) or once per note (its first tick's values).
- The replay's per-trigger control is held at the chosen rate before conversion (`hold_controls`), so track grouping,
  phrase paths and every lane follow it. `TickCtl` gains `row` (the row's first tick).
- Measured: Stash xz −22 % (row) and −38 to −42 % (note); row stays close to the reference, note loses risers, fades
  and tremor. Table in `docs/synth/unisynth.md` (Mini-cauldron XMs).

## Capabilities

### Modified Capabilities

- `synth-convert`: mini-cauldron XM import (control rate option).

## Impact

`crates/synth-convert` (`cauldron/import.rs`, `xm/replay.rs`, `main.rs`), `docs/synth/unisynth.md`. Default output
unchanged (byte-identical).
