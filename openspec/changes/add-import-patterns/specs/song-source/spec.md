# Spec Delta

## MODIFIED Requirements

### Requirement: Import from the song blob
Import SHALL read any blob `Song::parse` accepts and whose device kinds have parameter tables, and produce a song that
exports back to identical device chunks, track volumes, receives, device chains and automation bytes, with note
times converted from samples to ticks (rounded). By default each lane SHALL be cut into patterns on a 2-, 4- or
8-bar grid (the one with the least note data), never through a note: identical segments SHALL be one clip placed
many times, back-to-back repeats one looping placement, and a segment sharing at least half its notes with an earlier
pattern SHALL be named as its variant (`A.1`). The patterned song SHALL export to the same blob as the song with one
clip per lane placed at tick 0 (`--one-clip`). Unsupported kinds (for example legacy kinds 0–39) SHALL fail with an
error naming the kind and pointing to `synth-convert`.

#### Scenario: Blob round trip
- **WHEN** a blob of kind-64 devices is imported and exported again
- **THEN** device chunks, routing, volumes and automation bytes are identical and every note is within half a tick
  of its original sample

#### Scenario: Repeated phrase
- **WHEN** a lane plays the same 4-bar phrase four times and then a copy with one note changed
- **THEN** import makes clip `A` in one looping placement of 16 bars and clip `A.1` after it, and the export equals
  the one-clip export

#### Scenario: Legacy kind
- **WHEN** a WaveSabre blob with kind 1 (Slaughter) is imported
- **THEN** import fails naming kind 1 and `synth-convert`

## ADDED Requirements

### Requirement: Track names and descriptions
A song and each track MAY carry an `about` object (type, style, tempo/feel, engine, similar real-world instruments,
description). It SHALL be kept by load and save and SHALL NOT be part of the blob. The `--meta` file of `song import`
MAY carry `about` and `tracks` (`[{name, about?}]`, one per blob track before the master, in order); the importer
SHALL apply them and fail when the count differs.

#### Scenario: Named import
- **WHEN** `song import blob -o s.json --meta m.json` runs with three track entries for a blob of three tracks
- **THEN** the tracks carry those names and descriptions and the export equals the export without them

#### Scenario: Count mismatch
- **WHEN** the meta file lists two tracks for a blob of three
- **THEN** import prints the mismatch and writes nothing
