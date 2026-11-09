# Design

## Context

See proposal.md, Why. What already exists (change add-tf3-import, archived 2026-11-09):

- `tools/tf3-extract/unpack.py` emulates the kkrunchy stub and dumps the image. The Pandora dump is
  `~/Downloads/64k-tools/bc-intros/out/pandora.img`; the image base is 0x3c0000 for all three intros.
- `extract.py` reads the 2012 block: 32 presence bytes, parameter-major values in two variants (ranged and percent),
  and the song with 5-byte cells. It writes `.e3prj` with the 129 current names.
- `synth-convert e3prj` maps a current-format project in player or editor mode. `tools/unisynth-parity/tf3.ts` runs
  the capture check (`tools/tf3-parity/capture.ts`).
- The capture `~/Downloads/64k-tools/tf3-captures/pandora.wav` is 252.5 s, −17.5 LUFS, and the music starts at 7.50 s
  (YouTube RlyIJD9AEWM).
- The Enigma Studio 3 source release is one commit dated 2012-03-24, with no history, so it does not contain the 2011
  engine.

What the image shows so far:
- **Instruments:** 21 records of 100 bytes from 0x6ff1a. Bytes 0–97 look like parameters 0–97 of the current list.
  The last two bytes look like a value and the instrument index (0, 2, 3, …).
- **Song:** right after the instruments: `u32 1`, `u16 125`, `u8 8` + "New song", then `u16 109`, then
  `u32 16, 16, 8, 8`, then runs of 0/1/3 bytes. The patterns look column-split rather than stored as 5-byte cells.

## Goals / Non-Goals

**Goals:**
- Decode the format from the code that reads it, not from byte patterns alone.
- Change no current-format conversion (Transplant, Train Control and the presets stay byte-identical).

**Non-Goals:**
- Not a reference renderer for the 2011 engine, and no parity tighter than the capture check allows.
- No other 2011 Enigma intros, no editor or project format of the old Enigma Studio, and no unified-synth changes.
  A required engine change pauses the change for a proposal update.

## Decisions

### D1. Decode from the loader code, with byte patterns only as a cross-check
- **Approach:** Disassemble the unpacked image with capstone, run through `uvx --with capstone` like `unpack.py`.
- **Loaders:** Find the readers by their data accesses: the code that reads the block address 0x6ff1a and the song
  bytes after it. From those, follow the per-parameter uses in the voice, filter and envelope code. A parameter's
  meaning comes from the code that consumes it, compared against the 2012 engine's behavioural spec (functional
  spec §3–§6).
- **Alternatives:**
  - Guessing from byte statistics alone: it fails on columns that are constant in Pandora's 21 instruments.
  - Running the intro under Wine and logging: slow, and it gives no parameter meaning.

### D2. Translate in the extractor; converter option only for laws
- **Extractor:** `extract.py` gains an older-layout reader that writes a current-format `.e3prj`. Values are remapped
  to the current ranges, and parameters the old engine lacks are written at inert values.
- **Converter:** `synth-convert` only changes if a law differs, for example a different cutoff or envelope-time curve
  that no value remap can express. That law becomes an explicit option of the Tunefish 3 converter (e.g. `--engine
  2011`), off by default, so current projects keep identical output.
- **Laws the unified synth cannot play** (found in task 1.2; user decision 2026-11-09):
  - The four are the oscillator offset with its one-sided clip, per-layer clipping, the point curve evaluated without
    a table, and no decimation.
  - They are documented known differences, measured against the capture and named in the warnings.
  - Unified-synth features for them may come in a later change.
- **Alternative rejected:** a separate Rust reader for the old layout. It duplicates the song model for one song, and
  the image never enters the repository anyway, so the extractor is the right boundary.

### D3. Player mode and a Pandora capture item
- **Mode:** Pandora converts in player mode, as the released Transplant and Train Control were confirmed to be.
- **Effects:** the old format has no effect slots, so editor-only effects do not arise.
- **Parity:** `tools/unisynth-parity/tf3.ts` gains a Pandora capture item with the same criteria as the other two.
- **Offset:** the capture's 7.5 s silent start is handled by the existing coarse first-sound offset.

### D4. A go/no-go gate before any mapping code
- **When:** after decoding (tasks group 1) and a first render, before the extractor and docs work is polished.
- **Fail criteria:** the change stops if the song structure cannot be decoded completely, or if the first conversion
  misses the capture check by more than the 4 dB band limit with no identified cause.
- **On a stop:** the functional-spec notes stay and nothing is committed under `project/music/released/`. The tasks
  record why.

## Risks / Trade-offs

- [The old engine's DSP differs in ways the 2012 spec doesn't cover] → Each differing law is found through D1, and its
  effect on the capture bands is measured. Unexplained gaps are a no-go, not a tolerance change.
- [A capture-only reference is coarse: lossy audio, unknown gain] → Its loudness, bands and timing criteria are the
  ones that confirmed the Transplant and Train Control conversions. Per-instrument solos are used for diagnosis.
- [Constant columns across 21 instruments hide parameter meanings] → Meanings come from the code (D1), not from value
  variation.
- [The extractor parses third-party bytes] → Bounded reads and whole-parse acceptance, and no partial `.e3prj` is
  written. This covers the "Truncated image" scenario.

## Migration Plan

None. This only adds a song, and current conversions are unchanged by requirement. Rollback deletes
`pandora.song.json` and its `NOTICE.md` entry.

## Open Questions

- Do the last two bytes of each 100-byte record (a value, then what looks like the instrument index) hold anything
  sound-relevant? The loader settles it during decoding, and the plan is the same either way.
