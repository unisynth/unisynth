# Proposal

## Why

The music of The Black Lotus' Jizz (Wired '97) and Stash (The Party '97) is now fully understood
(`research/tbl-mini-cauldron/mini-cauldron-spec.md`): FastTracker 2 XMs whose samples are 16-step sequencer phrases
rendered at startup by Probe's "mini-cauldron" synth. The user wants these songs to be **ordinary real-time
`unisynth` songs**, like the WaveSabre, Tunefish and V2 imports. That means instruments play notes live, with no
precalculated phrase samples, so the songs can be edited in the DAW and compiled like any other track.

User decisions (2026-10-30):
- **Real time, not precalc.** Each mini-cauldron patch becomes a unified instrument. The 16 steps inside each phrase
  become notes in the song.
- **Sound-faithful, not bit-exact.** The 8-bit storage tricks were GUS-memory savers and are dropped: x87 24-bit
  rounding, 8-bit clamps between stages, 2:1/4:1 decimation, delta coding and stale buffer bytes.
  The 16-bit output clamp, phase wrap, aliasing oscillators and the compounding cutoff ramp are the instrument and stay.
- **Fidelity is measured** with the parity metrics already in use (loudness, octave bands, onsets), against the
  bit-exact reference.

Default taken (stated here so it can be overruled): the decimated phrases keep their original **bandwidth** through a
low-pass at the stored sample's Nyquist frequency. Stash's 4:1 phrases are about 4.4 kHz wide, and that darkness is
part of how they sound. Only the aliasing goes.

## What Changes

- **XM import**: `synth-convert xm <in.xm> <patches.json> <out.song>`. It reads a mini-cauldron XM and the decoded
  patch data (`research/tbl-mini-cauldron/ref/patches_*.json`), and replays the XM's control flow:
  - order list, speed/BPM, pattern break and jump, note delay and cut, retrigger, sample offset
  - volume column, volume and panning envelopes, fadeout, panning
  - pitch slides and tone portamento, finetune, relative note, and loops (forward and ping-pong)

  Each trigger of a generated sample is unrolled into the phrase's notes. Ties become mono-legato slides; each note
  carries its retrigger count and accent (one automation byte) and its phrase position (the velocity), and the
  instrument compounds the cutoff ramp from them. A phrase played at another pitch
  changes both its pitch and its step clock, as in the tracker. Stored samples (Stash drum kit, Jizz vocals)
  become sampler slots.
- **Unified instrument**: a new oscillator slot type **cauldron** (`us-osc-cauldron`). It covers the original naive
  16-bit-phase waveforms: saw, detuned saw pair, square/PWM, pulse, the spike wave in both versions, fold and noise,
  with phase reset at note-on, and the original resonant filter after it (base, envelope amount, decay, damping;
  a decay above 1 makes the cutoff rise).
- **Effects**: two new modes.
  - **comb** (`us-fx-flanger-comb`): a feed-forward difference comb plus an optional one-way delay sweep restarted
    by a parameter step. It covers v2's comb and doubler.
  - **wrap** (`us-fx-distortion-wrap`): drive and offset with modular wrap-around. It covers Stash's three
    wrap-distorted sounds.
- **Parity**: `tools/unisynth-parity` gains the three songs (Jizz, Stash song 1, Stash BLUISH) and a per-phrase
  check against the reference renders.
- **Docs**: a converter section in `docs/synth/unisynth.md`, and the port notes in the mini-cauldron spec point to it.

Out of scope:
- bit-exact output
- XM files not made by mini-cauldron (a general XM importer)
- committing the intros, XMs, converted songs or audio: they stay in the git-ignored `out/` as before
- size tuning for a compo build

## Discipline Skills

- `doubt-driven-review`: new oscillator type and effect modes freeze stored values once songs exist.
- `systematic-debugging`: parity gaps.
- `performance-optimization`: the speed and size report for the new modules.

## Capabilities

### New Capabilities

None. The importer is part of the existing converter capability.

### Modified Capabilities

- `synth-convert`: mini-cauldron XM import (timeline replay, phrase unrolling, instrument mapping, stored samples),
  and parity against the reference renders.
- `unified-synth`: cauldron oscillator slot type with its filter, and its feature.
- `unified-fx`: comb and wrap effect modes and their features.

## Impact

- `crates/synth-convert`:
  - new modules for the XM reader, the timeline replay and the mini-cauldron mapping
  - CLI `xm`
  - `serde_json` moves from dev-dependency to dependency, to read the patch JSON
- `crates/unisynth`:
  - `osc.rs` gets the new slot type; `fx/` gets `comb.rs` and `wrap.rs`
  - `module.rs` gets module ids appended; `Cargo.toml` gets features in the `us-osc`/`us-fx`/`us-all` groups
  - parameter values: the cauldron type takes an odd position of the expanded oscillator-type choice. Stored values
    of existing songs do not move.
- `tools/unisynth-parity`: a `cauldron` suite. References are rendered with `openmpt123` from the XMs that
  `research/tbl-mini-cauldron/tools/rebuild_xm.py` rebuilds. It skips when `out/` is absent.
- `research/tbl-mini-cauldron`: `run_all.sh` can hand the XMs to the converter.
