# crates/synth-convert (sidecar: converter rules and walkthrough)

Pull-only (not auto-injected). Details and mapping tables: `docs/synth/unisynth.md`, section Converter. Change add-unified-synth.

- **Structure.** Devices convert in place (same index); tracks, lanes, receives, volumes and device order are copied.
- **Effects.** Unified chunk = mode value + the legacy chunk unchanged; effect automation moves to parameter + 1.
- **Instruments.** Physical values are mapped through the unified laws. Automation lanes are re-mapped per affected unified parameter: continuous laws keep the points and add points where interpolation would be > 1 byte step off at a 100-frame sub-block start; discrete decodes become steps (two points at one frame). Structural parameters are never automated.
- **Tunefish.** Insert effects become devices after the instrument; automated routing (effect-slot kind, mod source/target) is an error; `.tfm` events land at 512-frame block starts (the legacy `f32` clock).
- **Elevated (add-elevated-import).** `music.asm` + the `music.h` beside it (`NUM_ROWS` strides the sequence). The machine tree runs as a stack of tracks: synth → track (pair slots, linear timed env 1 with sustain ½, pitch drop, capacity 1), effect → device on the top track (filter → EQ `svf-lfo`, sine/atan → distortion `sine`/`atan`, compressor → distortion `knee`, delay/allpass → delay `ping-pong`/`allpass`), mixer → a new track receiving both at the gains, the last track is the master. Note n at `(16r + s)·step`; `0x7F` → note-off of the sounding note (a linear env cuts); 0 holds. Stereo factor → voice pan `st²/(1+st²)` and master `vol·√(1+st²)`. Table detected by which walk ends on the terminator (`--table 7|8`). Errors: missing section, truncated machine, unknown opcode (with byte offset), pattern out of range, zero segment, stack under/overflow.
- **Output.** Written only after every device converted; errors name the problem and exit non-zero. Warnings (voice cap, unison over 16) go to stderr and do not stop the conversion.
- **Walkthrough.** The commands in the docs run as written (`FEATURES_DUMP=<dir>` makes the legacy test songs).
- **Errors.** Truncated or malformed song, a sample that is not GSM, unknown or not-yet-mapped device kind, an already unified device: message on stderr, non-zero exit, no output.
