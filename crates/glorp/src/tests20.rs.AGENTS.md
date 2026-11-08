# src/tests20.rs (sidecar: full detail of the AGENTS.md row)

Pull-only (not auto-injected).

- Synthetic only, ID 20 (21 tests, no song or tap data): rejection table + atomicity, 256-slot queue, offset-accurate onset, block-split bit identity, 16 voices / 17th lost, lowest-free reuse, 16 params latched at note-on (a later write changes the next note only), P4 inert, note-off inaudible, state persisting across notes of a voice (delay / follower / filters), determinism and note seeding, velocity ceiling (`MAX_VELOCITY` accepted, 214 refused, `noise_decay_exponent` < 0 up to it and > 0 above), independent f64 model of the whole voice (table sine kept for the shifter phase and click coefficients; `MODEL_REL` 5e-4) with negative controls.
