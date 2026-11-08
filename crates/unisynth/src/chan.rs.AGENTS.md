# src/chan.rs (sidecar: full detail of the AGENTS.md row)

Pull-only (not auto-injected).

- Channel strip after the voice sum (`us-strip`): `on(p)`, `ChanP::new(p, rate)` per control step, `Chan::{need,new,reset,process}`, `Sends`, `CHORUS_LEN`.
- Order: aux receive → DC filter (`ch::DC_FILTER`) → compressor → boost → dist/chorus by `ch::FXROUTE` (+ DC filter after dist) → sends (reverb/delay mono, aux A/B stereo) → ×0.6·vol.
- Quirk `exact_math` picks native atan in dist.

## History

- See change: add-v2-import, unify-v2-algorithms 6.5–6.8
