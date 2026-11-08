# crates/synth-convert (sidecar: converter rules and walkthrough)

Pull-only (not auto-injected). Details and mapping tables: `docs/synth/unisynth.md`, section Converter. Change add-unified-synth.

- **Structure.** Devices convert in place (same index); tracks, lanes, receives, volumes and device order are copied.
- **Effects.** Unified chunk = mode value + the legacy chunk unchanged; effect automation moves to parameter + 1.
- **Instruments.** Physical values are mapped through the unified laws. Automation lanes are re-mapped per affected unified parameter: continuous laws keep the points and add points where interpolation would be > 1 byte step off at a 100-frame sub-block start; discrete decodes become steps (two points at one frame). Structural parameters are never automated.
- **Tunefish.** Insert effects become devices after the instrument; automated routing (effect-slot kind, mod source/target) is an error; `.tfm` events land at 512-frame block starts (the legacy `f32` clock).
- **Output.** Written only after every device converted; errors name the problem and exit non-zero. Warnings (voice cap, unison over 16) go to stderr and do not stop the conversion.
- **Walkthrough.** The commands in the docs run as written (`FEATURES_DUMP=<dir>` makes the legacy test songs).
- **Errors.** Truncated or malformed song, a sample that is not GSM, unknown or not-yet-mapped device kind, an already unified device: message on stderr, non-zero exit, no output.
