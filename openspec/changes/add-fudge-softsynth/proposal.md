# Proposal

Follows `add-plain-xm-import`, which delivered the generator map, the shared conversion flow, the note byte contract
and `tools/xm-gen/maps/fudge-asc.json`.

## Why

Fudge's Alien Sex Clone (The Party 1998, 1st PC 64K; music by Simon) synthesized 22 of its 25 samples with two
generators. Its map names them `fm-shaper` and `sweep-drum`. No engine plays them yet, so the song cannot be
converted, and the plain-XM parity reports it as pending.

## What Changes

- **Engine oscillator types in unisynth** (design D3 of add-plain-xm-import):
  - Two types at the free odd positions 7 and 9, each behind its own feature, named by function: "Shaper tone" and
    "Sweep drum".
  - Both run through the clocked machinery: Catmull-Rom resampling and the reconstruction low-pass. Their engine
    clock comes from the slot's pitch, at 256 engine frames per cycle, so bend and transpose change the playback
    speed as they do for a sample.
  - At note-on a slot reads the byte contract: the start frame is `o::TRACKING`×4096 + `o::PM_OUT`×16, and a backward
    pass is `o::FEEDBACK` ≥ 128.
  - Seeking keeps a constant amount of state, because `unisynth` has no heap:
    - the tone is closed-form in the frame index, and it recovers its carried shaper value by scanning back;
    - the drum replays only its sweep from frame 0 and recovers the carried value from the last 64 frames; a
      backward pass plays from a 64-frame block of exact forward frames.
  - A note-on budget is measured (`performance-optimization`).
- **Fudge's two generators** use their nine shape values as the map's arguments, on exact power-of-two laws.
  Truncating x87 semantics are kept where they are audible, and the result is judged by the shared metrics, not
  bit-exactness. The carried shaper value at a call's entry is never audible: frame 0 always resets it. A test proves
  this.
- **Per-sample engine parity**: each generated sample's engine output at its native clock is compared with the local
  reference (the samples of `$ZENEKK_DIR/xm-gen/fudge-asc.reference.xm`). Then the song gets its row in
  `tools/unisynth-parity/plain-xm.ts`.
- **Publication**:
  - `project/music/released/fudge-alien-sex-clone.song.json`, with meta from `research/xm-softsynth-intros.md` §6:
    Simon / Simn, Demozoo 21494 and 202313, pouet 444;
  - a NOTICE entry;
  - the listening-page group "GUS 64K intros, 1998 (softsynth XMs)" in `tools/song-page/build.ts`;
  - a site rebuild.
- **DAW device panel**: an oscillator's fields follow its type:
  - its labels, and values in the engine's units;
  - only the common fields the type reads;
  - the type's own defaults when a slot switches to an engine type;
  - a drawing of the generated waveform.

## Discipline Skills

- `doubt-driven-review`: engine type positions and laws are frozen once songs use them.
- `performance-optimization`: the seek at note-on.
- `systematic-debugging`: parity gaps.

## Capabilities

### Modified Capabilities
- `unified-synth`: engine oscillator types, the clock from pitch, start position, backward pass and seeking.
- `synth-convert`: the Fudge engines' instruments, and per-sample parity.
- `synth-daw`: oscillator fields that follow the type, and the engine types' fields.
