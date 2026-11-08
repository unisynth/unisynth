# src/tests21.rs (sidecar: full detail of the AGENTS.md row)

Pull-only (not auto-injected).

- Synthetic only, ID 21 (19 tests): rejection table + atomicity (index, range, partial bound), queue limit, silence overwrite, offset-accurate onset, block-split bit identity, 16-voice polyphony / 17th lost / release frees the voice, envelope closed forms (attack / decay / sustain / release, level persists), latching at note-on (a later write does not change a sounding voice), seed sensitivity (one ulp of P7), pan, full-load finiteness, independent f64 model (ideal sines; measured MID 5.8e-5, SONG5 3.4e-4, SONG6 2.9e-5 relative; single-partial 1.4e-2).
