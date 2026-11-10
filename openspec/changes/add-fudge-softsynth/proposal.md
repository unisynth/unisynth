# Proposal (stub)

Follows `add-plain-xm-import`, which delivered the generator map, the shared conversion flow, the note byte contract
and `tools/xm-gen/maps/fudge-asc.json`. Status: stub; design, specs and tasks to be written.

## Why

Fudge's Alien Sex Clone (The Party 1998, 1st PC 64K; music by Simon) synthesized 22 of its 25 samples with two
generators. Its map names them `fm-shaper` and `sweep-drum`; no engine plays them yet, so the song cannot be converted
(the plain-XM parity reports it pending).

## What Changes

- **Engine oscillator types in unisynth** (design D3 of add-plain-xm-import): a type per engine at a free odd
  position, behind its own feature, run through the clocked machinery (phrase clock, Catmull-Rom, reconstruction
  low-pass). Note-on reads the byte contract: start frame `o::TRACKING`×4096 + `o::PM_OUT`×16, backward pass
  `o::FEEDBACK` ≥ 128. Seek-by-replay for engines with carried state (render from frame 0 into scratch at note-on, a
  per-voice last-seek cache), with a measured note-on budget (`performance-optimization`).
- **Fudge's two generators** as the first two types (named by function: a tone with a two-stage shaper, a sweep
  drum), reading the map's `params` (`frames`, nine raw arguments, the drum's carried shaper value). Truncating x87
  semantics where they are audible; judged by the shared metrics, not bit-exactness.
- **Per-sample engine parity**: each generated sample rendered by its engine against the local reference
  (`$ZENEKK_DIR/xm-gen/fudge-asc.reference.xm`'s samples); then the song row of `tools/unisynth-parity/plain-xm.ts`.
- **Publication**: `project/music/released/fudge-alien-sex-clone.song.json` (meta from `research/xm-softsynth-intros.md`
  §6: Simon / Simn, Demozoo 21494 and 202313, pouet 444), NOTICE entry, listening-page group "GUS 64K intros, 1998
  (softsynth XMs)" in `tools/song-page/build.ts`, site rebuild.
- DAW device panel: the engine types' fields.

## Discipline Skills

`doubt-driven-review` (engine type positions and laws are frozen once songs use them), `performance-optimization`
(seek-by-replay at note-on), `systematic-debugging` (parity gaps).

## Capabilities

### Modified Capabilities
- `unified-synth`: engine oscillator types, start position, backward pass.
- `synth-convert`: the Fudge engines' instruments; per-sample parity.
- `synth-daw`: the engine types' device fields.
