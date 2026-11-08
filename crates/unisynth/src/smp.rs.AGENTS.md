# src/smp.rs (sidecar: full detail of the AGENTS.md row)

Pull-only (not auto-injected).

- Sampler slot (port of WaveSabre `SamplePlayer`): `Source` (`None`, decoded GSM `F32` in the pool, DLS `Wave` read in place), `Sample` (`mono`, `wave`, optional `right` partner via `Dls::partner` with `us-sample-stereo`), `Loop`, `Interp`, `Cfg::from_shape` (original clamps, per control segment), `State` (`start`, `next`, `next_pair`; nearest/linear, repeat/ping-pong, ends outside the data;
- `pos` with `probe`), `pool_need` + `load` (GSM decoded once per distinct index, DLS wave `idx − 1` with its `wsmp` loop; no bank/bad index = `None`; missing feature → `UnsupportedModule`), `pick(inst, key, vel, rate)` → region `Sample` + `Adj { root, rate }`, `Insts`, `choice_of`.
- Position advances `2^((note − 60)/12)` sample units per output frame at any rate.

## History

- See change: add-gmplus-bank
