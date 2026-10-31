# Proposal

## Why

The three converted TBL soundtracks (Jizz "Smokeout", Stash "over2" and "bluish", music by Probe) should open in the
DAW like the released WaveSabre songs. They did not import: the song tables lacked the cauldron's two effect modes,
one echo time was stored as 1.03 (outside a parameter's 0..1), and the import turned the converter's legato (a note-on
before the old note's note-off at one sample) into a restart, since song/1 plays note-offs first at one sample.

## What Changes

- **Song tables** gain Distortion Wrap (`drive`, `offset`) and Flanger Comb (`delay`, `amount`, `sweep`, `restart`);
  a test checks that every effect mode the engine plays has a table.
- **Import keeps same-sample on-before-off.** A note ended at the sample where another note started just before it
  lasts one tick longer, so the export plays the new note first again.
- **Converter:** echo bytes capped at 1 (the top, 16 quarter beats and 200 ms, decodes exactly); `synth-convert xm …
  --names <tracks.json>` writes track names (`<sample> (record r, ±class)`, `<sample> (sample)`).
- **Demo songs:** `research/tbl-mini-cauldron/tools/make_demos.py` writes `tbl-jizz-smokeout`, `tbl-stash-over2`,
  `tbl-stash-bluish` with meta to `project/music/released/` (git-ignored, no licence, like the WaveSabre ones).

## Capabilities

### Modified Capabilities

- `song-source`: import from the song blob (same-sample order).
- `synth-convert`: mini-cauldron XM import (`--names`, parameters in range).

## Impact

`crates/song` (`import.rs`, `tables.rs`), `crates/synth-convert` (`cauldron.rs`, `cauldron/import.rs`, `main.rs`),
`research/tbl-mini-cauldron/tools/make_demos.py`, `project/music/NOTICE.md`, `docs/synth/unisynth.md`. Through song/1
the songs render within ~0.1 dB median and 4.3 dB worst per octave band and second of the converter's blob (notes on
the 960-tick grid). Existing demo songs: unchanged (their tests pass).
