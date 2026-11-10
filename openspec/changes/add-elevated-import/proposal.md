# Proposal

## Why

Mentor's (TBC) 4k synthesizer, shipped in **Elevated** (TBC & RGBA, Breakpoint 2009, 4k winner) and reused for TBC's
**Atrium**, is the smallest scene synth we have source for. Its song file (`music.asm`) is a generated NASM data file:
a tempo block, 16-step patterns, a per-channel order list and a *machine tree*, which is a stack program of synths and
global effects. `~/Downloads/zenekk/TBC - Atrium/` holds the synth (`synth.asm`, about 400 lines of x87 code), Atrium's
`music.asm`/`music.h` and a debug build (`hack.exe`). Elevated's own `music.asm` comes with iq's public source release.

Importing these songs into `unisynth` follows the V2 and Tunefish 3 pattern: the songs then play in the DAW and on the
site, and the algorithms become modes that any song can use.

The engine is simple, but several of its algorithms do not exist in `unisynth` yet:
- detuned oscillator pairs combined into the voice by add, subtract or multiply
- a pitch drop towards a fixed base frequency
- a linear 4-segment envelope with a fixed sustain level of ½
- a Chamberlin SVF whose cutoff two song-time sine LFOs move (the filter step itself already exists; the effect
  placement, the one-step rate and the LFOs do not)
- `atan` and `sin` waveshapers and a memoryless knee compressor
- ping-pong feedback delay and allpass lines that swap channels on every pass

User decisions (2026-11-10):
- **Import into unisynth.** There is no standalone legacy crate; the original assembly is only the reference renderer.
- **Reference:** the original `synth.asm` + `music.asm`, assembled with `nasm -f win32` and linked to our MIT harness,
  then run under Wine. It renders WAVs outside the repo.
- **Acceptance songs:** Atrium (from `~/Downloads/zenekk`) and Elevated (from the public source release).

## What Changes

- **Import:** `synth-convert elevated <music.asm> <out.song> [tail-seconds]`.
  - It parses the NASM data (`_tempo_data`, `_pattern_data`, `_sequence_data`, `_machine_tree`) and runs the machine
    tree symbolically.
  - Each synth machine becomes a track with a unified instrument (kind 64) and its notes. Effects become inserts on the
    track at the top of the stack. A mixer machine becomes a new track that receives both stack entries with their
    gains. The last stack entry becomes the master.
  - Timing: one step every `MAX_NOTE_SAMPLES` frames, 16 steps per pattern. Note `0x7F` cuts; note 0 holds.
- **Unified instrument, new settings,** each behind its own feature. Encodings are appended or placed at free
  positions, so existing chunks keep their bytes.
  - `pair` oscillator slot type (`us-osc-pair`): sine, square or saw (naive). Two copies at phase ratios `2 − d` and
    `d` plus a phase shift, and a polarity field (an added, negated pair subtracts).
  - Pitch drop (`us-voice-drop`): the frequency falls as `base + (note − base)·k^n` towards a fixed base. It is
    evaluated at control points (appended voice fields).
  - Linear timed envelope (`us-env-linear`): appended per-envelope `linear` and `hold` fields. Attack, decay, hold and
    release are linear and run by themselves; a note-off cuts. That makes Elevated's `0x7F` cut a plain note-off.
  - The mono hard cut is the existing capacity 1 with steal oldest.
  - No existing choice is expanded (user decision 2026-11-10).
- **Unified effects, new modes:**
  - EQ kind 67 `svf-lfo` (`us-fx-eq-svf-lfo`): Chamberlin low/high/band output with a dry mix. It reuses the shared
    state-variable step in `flt.rs` (the same equation as the V2/WaveSabre filter) with a new one-step form. The cutoff is moved by
    two sine LFOs that run in song time and are computed from the absolute frame, so seeking works.
  - Distortion kind 65 `atan` (the V2 overdrive arithmetic), `sine` and `knee` (`us-fx-distortion-atan`, `-sine`, `-knee`).
  - Delay kind 68 `ping-pong` and `allpass`, with crossed stereo feedback and lengths in frames
    (`us-fx-delay-pingpong`, `-allpass`).
- **Reference and parity:**
  - `tools/elevated-parity/` holds `build-ref.sh`, the MIT harness `render-ref.c` (with `--solo <channel>`) and
    `parity.ts`, which uses the shared metrics. It skips cleanly (exit 0) when the sources are absent.
  - `research/elevated-synth.md` records the behavioural reading of the engine.
  - `docs/synth/elevated.md` documents the format and the mapping.
- **Size:** the new features join the feature-set gate and the size report.

Out of scope:
- bit-exact output
- the original's 16-bit overflow wrap (`fistp` stores −32768 past full scale); we clamp
- publishing the songs on the site
- the four sample-based `.xm` modules in the same folder
- committing any original code, song or WAV

## Discipline Skills

- `doubt-driven-review`: the oscillator-type position, the appended parameter indices and the effect-mode ids are
  frozen once songs exist.
- `systematic-debugging`: parity gaps against the reference.
- `performance-optimization`: the per-feature size cost and render speed.

## Capabilities

### New Capabilities
None. Elevated support extends existing capabilities.

### Modified Capabilities
- `unified-synth`: the pair oscillator type (with polarity), pitch drop, the linear timed envelope, the mono hard cut.
- `unified-fx`: the `svf-lfo` EQ mode, the `atan`/`sine`/`knee` distortion modes, the `ping-pong`/`allpass` delay
  modes.
- `synth-convert`: the `elevated` import, machine-tree routing, note rules, reference parity, local material.
- `synth-daw`: effect modes decoded by stored position; Pair, drop and linear envelope fields in the device panel.

## Impact

- Code:
  - `crates/unisynth`: osc, voice, env, fx modes, params, features
  - `crates/synth-convert`: new `elevated` module and command
  - `tools/check-unisynth-features.sh`
  - new `tools/elevated-parity/`
- Docs:
  - new `docs/synth/elevated.md`
  - `docs/synth/unisynth.md` (modes, provenance)
  - the directory `AGENTS.md` files
  - new `research/elevated-synth.md`
- Licensing: the Elevated synth is CC BY-NC-SA 3.0. Its behaviour is reimplemented in our MIT code; we copy no code,
  comments or tables. The original assembly is assembled only by the external script into `/tmp`. The songs are © their
  authors and stay local.
- Size: a song that uses none of the new settings builds without them.
