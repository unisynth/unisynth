# Proposal

## Why

Two of Brain Control's three Tunefish 3 intros are now in the song library (change add-tf3-import). The third,
Pandora (Revision 2011, music by Skyrunner), was made with an older Tunefish 3. `tools/tf3-extract/extract.py` cannot
find its synth block, so the song cannot be converted. Nobody published the source of that engine version. The only
ways in are the released executable and a YouTube capture.

## What Changes

- Reverse-engineer the older format from the unpacked Pandora image (`tools/tf3-extract/unpack.py` already produces
  it). Disassemble the intro's song and instrument loaders and its synth parameter use. Record the result in
  `research/tunefish3-functional-spec.md` as an "older engine" section: the parameter list, the song encoding and any
  effects or laws that differ from the 2012 engine. Known so far: 21 instruments × 100 bytes stored instrument-major,
  whose first 98 bytes follow parameters 0–97 of the current list, and no effect slots. The song starts with
  `id 1, bpm 125, u8 name length, "New song"`, followed by pattern data that does not use the current 5-byte cell layout.
- A go/no-go gate after the decoding. If the format, or the sound of the older engine, cannot be recovered well enough
  to pass the capture check, the change stops there. The research notes are kept, and no song is committed.
- Extend `tools/tf3-extract/extract.py` with the older layout. It writes a current-format `.e3prj`, mapping the old
  parameters onto the current ones; parameters the old engine lacks are set so that they have no effect.
- If the older engine's sound differs in ways a value mapping cannot express, add a matching option to the Tunefish 3
  converter in `synth-convert`. No changes to the unified synth are planned. If one turns out to be needed, the change
  pauses for a proposal update.
- Judge the conversion against the Pandora capture with the existing capture check. There is no reference renderer for
  this engine version.
- Commit `project/music/released/pandora.song.json` via `tools/tf3-extract/make-demos.py`, credited to Skyrunner, and
  list it in `project/music/NOTICE.md`, published pending permission like the other released songs.

## Discipline Skills

`doubt-driven-review` (each format interpretation before the extractor and the mapping rely on it),
`systematic-debugging` (capture mismatches), `security-hardening` (the extractor parses bytes from a third-party
executable: bounded reads, no partial results).

## Capabilities

### New Capabilities

(none)

### Modified Capabilities

- `synth-convert`: the Tunefish 3 import gains the older Pandora-era format (extraction, parameter mapping, capture
  acceptance, the released song).

## Impact

- `tools/tf3-extract/` (`extract.py` older-layout reader, `make-demos.py` Pandora entry), `tools/unisynth-parity/tf3.ts`
  (Pandora capture item), possibly `crates/synth-convert/src/tf3.rs` (an option for the older engine's laws).
- `research/tunefish3-functional-spec.md`, `docs/synth/tunefish3.md`, `project/music/NOTICE.md`, per-directory
  `AGENTS.md` rows.
- New committed song `project/music/released/pandora.song.json`, plus the site rebuild that publishes it.
- No new dependencies in the repository. Disassembly uses capstone through `uvx`, outside the build, as unpacking
  already does with unicorn.
