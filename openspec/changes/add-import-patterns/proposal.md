# Proposal

## Why

`song import` turns every blob lane into one clip placed once at tick 0, so a converted song (the WaveSabre and
Tunefish reference songs in `project/music`) shows each track as one pattern as long as the song, named `Track N`. A
DAW user cannot see the song's structure, edit a repeated phrase once, or tell what a track is. The originals had
patterns, track names (WaveSabre `Refs/*.ref` `Tracks[].Name`) and recognisable instruments.

## What Changes

- **Pattern detection in import (default on).** Per lane, the importer tries 2-, 4- and 8-bar grids, cuts the lane
  into segments at grid lines no note crosses, keeps identical segments as one clip placed many times, marks a
  segment that shares at least half of its notes with an earlier pattern as a **variant** of it (`A.1`, `A.2`), merges
  back-to-back repeats into one looping placement and picks the grid with the least note data. Clip ids are
  `<track id>/<name>` (the DAW shows `A`, `B`, `A.1`). The export is byte-identical to the one-clip import.
  `--one-clip` keeps the old behaviour.
- **Import config.** The `--meta` file may also carry `about` (the song) and `tracks` (`[{name, about}]` in blob track
  order); the importer names the tracks and attaches the descriptions. The count must match.
- **Descriptions in `song/1`.** Optional `about` on the song and on each track: type (genre or track role), style,
  tempo/feel, the synth engine, similar real-world synthesizers or drum machines and free text. Kept by load and save,
  never exported.
- **Demo songs.** `tools/demo-songs/meta/<name>.json` holds the track names (WaveSabre: from the original project's
  `.ref`; Tunefish: none upstream, named from the analysis) and the descriptions for the six reference songs;
  `tools/demo-songs/build.ts` merges them into the import config. `tools/demo-songs/names.ts` refreshes names from
  the WaveSabre checkout.

## Discipline Skills

`doubt-driven-review` (the import change must keep the exported blob byte-identical), `code-simplification`.

## Capabilities

### Modified Capabilities

- `song-source` (change `add-synth-daw`): import requirement (patterns instead of one clip), metadata (track names,
  `about`).

## Impact

`crates/song` (`import`, new `patterns`, `model`, `validate`, `main`), `schema/song.schema.json`, `packages/daw`
(`model.ts` keeps `about`), `tools/demo-songs`, `project/music/*.song.json` (rebuilt). Depends on `add-synth-daw`
(archive it first or together).
