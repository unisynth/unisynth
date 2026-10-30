# Proposal

## Why

A song converted from a tracker has many sparse tracks: Stash song 1 converts to 119, which sound 12 % of the time
(Jizz 15 %, BLUISH 21 %). The mixer ran every track's devices every sub-block, so song 1 rendered slower than real
time (206.9 s for 203.7 s on one thread). Coarser track grouping or thinner automation in the converter bought under
10 % and cost sound; skipping silent tracks in the player keeps the output.

## What Changes

- **Quiet-track skip in the mixer (`synth-core`).** A track sleeps (its devices do not run, its buffers stay zero) in
  a sub-block with no delivered event, an exactly zero input, and every device quiet for the chain's memory: each
  device's output and the input below −120 dBFS for at least the sum of the devices' `quiet_after()` (and 4096
  frames). Automations still run while it sleeps; before it runs again each device gets `wake(skipped)`. Steps of a
  track: events → clear + receives → sleep decision → automations → devices → volume (receives move before the
  automations; the two are independent).
- **Device contract.** `Device::quiet_after() -> Option<u32>` (default `None`: unknown, never skipped),
  `quiet_skip() -> bool` (a constant; default false compiles the skip path out), `wake(skipped)` (default nothing).
  `Either` forwards them; `Player::set_skip_quiet` / `Mixer::set_skip_quiet` turn the skip off (default on).
- **unisynth answers (`us-quiet`, in `us-all`).** The instrument is quiet without voices, queued events, channel
  stage or speech; shaper, EQ 3band and wrap without offset have no memory; tempo delay and comb their line capacity.
  EQ 3band's `wake` restarts its gain ramp from the parameters set while asleep, the comb's advances its sweep counter.
  Every other mode answers unknown. A speed option, not a module: no song needs it, `synth-convert features` does not
  list it.

## Discipline Skills

`performance-optimization` (measured before and after), `doubt-driven-review` (a shared mixer step for every engine).

## Capabilities

### Modified Capabilities

- `synth-mix`: track render steps (receives before automations, the sleep decision), quiet tracks sleep.
- `unified-synth`: quiet-track answers of the instrument and effect modes, feature `us-quiet`.

## Impact

- `crates/synth-core`: `device.rs`, `mix/mixer.rs`, `mix/mixer/par.rs`, `mix/player.rs`, `mix/registry.rs`.
- `crates/unisynth`: `device.rs`, `instrument.rs`, `fx/{mod,three_band,comb,tempo,wrap,shaper}.rs`, `Cargo.toml`,
  `examples/render.rs` (`US_NO_SKIP`).
- Measured: Jizz 76.1 → 32.6 s, BLUISH 51.3 → 32.6 s, song 1 206.9 → 88.1 s; output within one 16-bit step of the
  unskipped render. Size: songs without `us-quiet` +512 raw / +128 xz / 0 squishy; Jizz with it +484 xz / +512
  squishy. Other engines (WaveSabre, Tunefish, Glorp, V2 modes) answer `None`: unchanged.
