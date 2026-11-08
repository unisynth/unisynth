# src/speech.rs (sidecar: full detail of the AGENTS.md row)

Pull-only (not auto-injected).

- `Ronan` (`us-speech`), port of V2 `ronan.cpp` via v2redux: `PHONEMES` 69×19 table, `SYLS` 52 syllables;
- `Ronan::{new(texts, rate), tick, process, note_on, note_off, set_ctl}`; texts NUL-separated from the chunk's text section;
- CC4 text/rate, CC5 pitch;
- `!`/`_` wait for note on/off.

## History

- See change: add-v2-import
