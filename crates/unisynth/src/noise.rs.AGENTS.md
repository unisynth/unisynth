# src/noise.rs (sidecar: full detail of the AGENTS.md row)

Pull-only (not auto-injected).

- Noise source: `Noise` (per-voice xorshift, `start(seed)` at note-on, `next(level, band, width)`: stereo uniform ±1; width 1 independent channels, 0 same noise, between at equal power; band = 4-pole low-pass `LP` (`Kind::Pole4Lp`) then RBJ high-pass `HP` at resonance `BAND_RES` 0.05 on `flt::Slot`s).
- Tests: band path equals the Tunefish filters on the same samples; distribution;
- `us-noise` off names the module when a patch sets or routes the level.

## History

- See change: unify-v2-algorithms 2.2
