# Proposal

## Why

The intro needs music, and we will run two synth engines: WaveSabre (Logicoma, MIT) and our own Tunefish-4-compatible
engine (`add-tf-synth`). Comparing the two sources (`research/softsynth-research.md`, "Tunefish 4 + WaveSabre") showed
that about 30% of WaveSabre and 20% of Tunefish is the same machinery:
- Freeverb comb and allpass filters, circular delay lines, the standard biquad filter, maths helpers, denormal protection;
- voice handling, note-event timing, the device interface;
- WaveSabre's song layer (tracks, effect chains, sends, automation), which can express everything a Tunefish song does.

WaveSabre is MIT, renders natively on our macOS dev box (measured 5–20× real time), and has real reference songs. So we
port it first, and build the shared layer while doing it. The Tunefish change then plugs into a finished shared core
instead of keeping private copies of the same primitives.

The user chose two levels of combination (decision 2026-10-25):
1. One song can hold instruments from both engines (a shared song/mixer format).
2. Every effect can be used on any track.

A merged "super instrument" is deferred to its own later change.

## What Changes

- New `no_std`, f32 crate **`crates/synth-core`** (MIT, depends on `libm` only), the shared layer:
  - DSP primitives: Freeverb comb/allpass, a circular delay with integer and fractional reads, a Direct Form I biquad
    with separate coefficient designers, a one-pole filter, maths (note→Hz, dB, pan law, clamp, mix, exp2) on `libm`,
    and a denormal guard for x86 and AArch64.
  - Framework: the `Device` trait (instruments and effects behind one interface: notes with sample offsets, normalized
    parameters, block render with main and sidechain inputs), a voice pool with selectable policies, a bump `Pool` over
    caller memory, and an `Rng` trait so each engine keeps its own random law.
- **`synth-core` mixer and song format (`synth-mix`)**: a track graph (device chains, sends/receives into main or sidechain
  inputs, track volume, linear parameter automation, dependency-ordered rendering, master = last track). Its binary song
  format is WaveSabre's song blob, so WaveSabre songs load unchanged. Device kinds are a numeric registry: ids 0–12 are
  WaveSabre's; later engines add theirs.
- New `no_std`, f32 crate **`crates/wavesabre`** (MIT, a direct port that keeps Logicoma's copyright notice):
  - Instruments: Falcon, Slaughter, Specimen, Adultery (GM.DLS through `crates/dls`), Thunder (deprecated, for old songs).
  - Effects: Scissor, Leveller, Crusher, Echo, Smasher, Chamber, Twister, Cathedral.
  - A `no_std` GSM 06.10 (WAV49) decoder ported from libgsm (permissive license, notice kept) for the sample instruments.
  - Each device is a cargo feature (`ws-<device>`, default `ws-all`), so only the devices a song uses get linked.
- Verification tooling: an `examples/render.rs` (std) song-to-WAV renderer, an opt-in `tools/ws-parity/` that builds
  upstream WaveSabre natively on macOS (the lab method: pthread `Windows.h` shim + libgsm, no Docker or Wine) and compares
  renders, and a compo size report.

Out of scope:
- Multithreaded rendering. The renderer is single-threaded and deterministic; threads come later as their own change.
- Audio device output and timeline sync in the player.
- The editor and VST authoring (musicians keep using the upstream WaveSabre VSTs and converter).
- Tunefish (stays in `add-tf-synth`, which is rebased onto this change).
- The merged instrument (option 3).
- 64klang.

## Discipline Skills

- `doubt-driven-review`: the `Device` trait and the shared song format are public contracts for every later engine.
- `performance-optimization`: render speed for prerendering, and the compo code-size budget.
- `systematic-debugging`: parity mismatches against the upstream render.
- `security-hardening`: the song decoder and GSM decoder parse untrusted bytes and must reject malformed input without
  panicking.

## Capabilities

### New Capabilities
- `synth-core`: shared DSP primitives, the `Device` trait, voice pool, event timing, memory pool, determinism rules.
- `synth-mix`: the shared track/mixer model, automation, the device-kind registry, and the binary song format (WaveSabre
  song blob compatible).
- `ws-synth`: the WaveSabre device set (instruments, effects, GSM samples, GM.DLS sampler), its parameter and chunk
  compatibility, reference parity and size report.

### Modified Capabilities
- (none) `gm-sound-bank` is used as-is by the Adultery port.

## Impact

- New crates `crates/synth-core` and `crates/wavesabre`, added to workspace `default-members`. `wavesabre` depends on
  `synth-core`, `libm` and `crates/dls`.
- New `tools/ws-parity/` (scripts plus our MIT harness. The upstream checkout, reference WAVs and third-party songs stay
  in `~/Downloads/64k-tools/synth-lab` or `/tmp` and are never committed.)
- `add-tf-synth` is updated to depend on this change. It will use `synth-core` primitives and implement `Device`, so
  Tunefish instruments and effects can be placed on `synth-mix` tracks.
- Docs: `crates/AGENTS.md`, `docs/architecture.md` (audio section), `research/softsynth-research.md`.
- Licensing: everything ships MIT. `crates/wavesabre` carries the WaveSabre MIT notice; the GSM module carries the libgsm
  notice.
