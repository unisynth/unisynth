# Tasks

## 1. Decode the older format

- [x] 1.1 Disassemble the unpacked Pandora image with capstone (`uvx --with capstone`), locating the instrument and song
  loaders through their accesses to the block at 0x6ff1a and the song after it. Write their structure (record size,
  field order, pattern and instance encoding) into a new "Older engine (Pandora, 2011)" section of
  `research/tunefish3-functional-spec.md`. Verify by hand-parsing the whole song with the documented layout: every
  pattern and instance read, ending exactly at the block's end.
- [x] 1.2 Trace the voice, filter, envelope and LFO code that consumes each instrument byte. Record a per-byte table
  (old byte → current parameter, value conversion, or "differs: <law>") in the same section, including the last two
  bytes of each record (design Open Question). Verify that every one of the 100 bytes has a row, and that each "same as
  2012" row cites the matching code in both engines.
- [x] 1.3 Go/no-go (design D4): write a throw-away `.e3prj` from the 1.1–1.2 tables, convert it in player mode and run
  the capture check against `pandora.wav`. Record the result and the decision in this file. On no-go, stop the change
  after documenting why in `docs/synth/tunefish3.md`.
  - **Result (2026-11-09):** the song decodes to 245.8 s and renders with today's converter.
    - *Raw 2012 laws* fail on one band: 8 kHz +4.40 dB (bands 63 Hz–4 kHz within 2.73 dB, timing in 8/8 windows).
    - *2012 values remapped to approximate the old laws* pass: every band within 3.95 dB, timing in 8/8 windows.
      Remapped: unison to the nearest reachable layer count, the sub-oscillator on, drive `p^(1/3)`, the LFO rate
      `× √π`, and the LFO depth and route amount so the 2012 sine gives `0.5 + 0.5p · sin`; band-pass and notch off.
    - Both use a capture offset of 7.49 s and a normalisation gain of 9.6–9.9 dB, like Train Control's YouTube
      normalisation.
    - Not modelled in either: the OscOffset with its upper-only clip, per-layer clipping, the 3-pass
      sub-oscillator and 6-layer unison, and the missing decimation stage.
  - **Decision: go.** Group 3 implements the old laws as a converter option, so they no longer depend on approximate
    value remaps. The unmodelled items stay listed as known differences unless 4.1 needs them.

## 2. Extractor

- [x] 2.1 Add the older-layout reader to `tools/tf3-extract/extract.py` with bounded reads and whole-parse acceptance.
  It writes a current-format `.e3prj` with inert values for missing parameters and all effect slots off. Verify that
  Pandora yields one block (bpm 125, all instruments and the decoded patterns), and that Transplant and Train Control
  `.e3prj` output is byte-identical to before (`cmp`).
- [x] 2.2 Add `tools/tf3-extract/extract.test.ts`, which runs on a synthetic image so no third-party bytes are
  committed. Cover an older-layout block parsed with its song, a truncated block rejected with no file written, and
  current-format blocks still found. Verify with `bun test tools/tf3-extract`.
- [x] 2.3 Update the `tf3-extract/` row in `tools/AGENTS.md` and the extraction section of `docs/synth/tunefish3.md`
  with the older layout. Verify that the module docstring and the docs name the same layout.

## 3. Converter (only for laws a value cannot express)

- [x] 3.1 For each "differs" row from 1.2 that the converter can reproduce (unison, sub-oscillator passes, drive, LFO
  sine and rate), write a failing test in
  `crates/synth-convert/src/tf3.rs`, then implement it behind an older-engine option, off by default. Verify with
  `cargo test -p synth-convert`, and confirm Transplant, Train Control and all 53 presets convert byte-identically
  (`cmp` against outputs saved before the change). If 1.2 has no such rows, record "none needed" here and check the box.
- [x] 3.1a Measure each known difference the unified synth cannot play (offset with one-sided clip, per-layer clip,
  table-free curve, no decimation): its effect on the Pandora capture bands, from an original-law simulation of the
  affected instruments or by elimination. Record the numbers in the functional spec §10 and `docs/synth/tunefish3.md`,
  and have the older-engine option warn by name. Verify that converting Pandora lists the warnings.
- [x] 3.2 Document the option (if any) in the `synth-convert` CLI help, `crates/synth-convert/AGENTS.md` and
  `docs/synth/tunefish3.md`. Verify that the help text runs as documented.

## 4. Parity and the released song

- [x] 4.1 Add a Pandora capture item to `tools/unisynth-parity/tf3.ts`, which skips when the capture is absent. Verify
  that `bun tools/unisynth-parity/tf3.ts` passes it (bands within 4 dB, timing ≥ 90 % of windows), that it skips with
  `TF3_CAPTURES` pointing at an empty directory, and that `tf3-report.md` lists the Pandora numbers.
- [x] 4.2 Add Pandora to `tools/tf3-extract/make-demos.py` (meta: Pandora by Brain Control, music by Skyrunner, Pouet
  56868, the capture parity) and write `project/music/released/pandora.song.json`. Verify that it loads in the DAW
  and that the material scan in `tools/unisynth-parity/tf3.test.ts` still passes (no image, exe or capture tracked).
- [x] 4.3 Add Pandora to `project/music/NOTICE.md` and the song sources in `docs/synth/tunefish3.md` (replacing "not
  converted"). Verify that the NOTICE entry names Skyrunner and says it is published pending permission.

## 5. Integration

- [x] 5.1 Run `cargo test --workspace` and `bun test`, check that the bake check is unchanged, and run
  `openspec validate add-tf3-pandora --strict`. All must pass.

## Workflow follow-up

- Rebuild and publish the site (`scripts/build-daw.sh`, `scripts/build-songs.sh`) from a tree that holds only this
  change, as with add-tf3-import.
- Archive the change.
