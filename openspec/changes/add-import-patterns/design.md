# Design

## Context

Import reads a `synth-mix` blob (absolute sample times per lane) and writes `song/1`. Notes are rounded to ticks
(960 per beat). Export flattens placements back into one lane per track (`export::flatten`: placements sorted, loops
repeat every clip length, notes cut at placement end).

## Decisions

### P1. Exact first, structure second

The patterned song must export to the same blob as the one-clip song. Only notes move between clips, never in absolute
time, and a segment is never cut through a note: a grid line inside a note (`start < line < start + len`) is not a cut
point and the segment grows to the next free line. A note ending on the line is fine (it ends inside its clip).

### P2. Grid choice per lane

For each of 2, 4, 8 bars (bar = `960·4·sig0/sig1` ticks): segments → distinct clips → placements (empty segments
dropped, back-to-back repeats of a full-length clip merged into one looping placement). Cost = notes stored in
distinct clips + 4 per placement; least cost wins, ties go to the longer grid. The last segment's placement ends at
the song length (its clip keeps the grid length).

### P3. Variants

A distinct segment is a variant of an earlier base pattern of the same length when at least half of the notes of the
larger one match by (start within ±`TOL` ticks, pitch) — `TOL` = 1/64 bar, so block-jittered Tunefish copies still
group. The best-matching base wins; variants of a base are numbered in order (`A.1`, `A.2`). Non-variants get the
next letter (`A`…`Z`, `AA`…). Naming only: a variant is its own clip.

### P4. Shared lanes

Several tracks may play one lane. The lane is split once; every track playing it gets the same placements (linked
clips). Clip ids use the first such track's id.

### P5. Config, not new flags

Names and descriptions come through the existing `--meta` file: the importer takes `about` and `tracks` out and reads
the rest as `Meta`, so old meta files still work. `about` objects are typed (`deny_unknown_fields`) so a typo fails.

## Risks

- Tunefish lanes: legato notes often cross grid lines by a few ticks, so segments grow; the result is still exact.
