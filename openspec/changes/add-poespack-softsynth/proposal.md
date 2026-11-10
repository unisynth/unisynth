# Proposal (stub)

Follows `add-plain-xm-import` and `add-fudge-softsynth` (which brings the engine oscillator types). Status: stub;
design, specs and tasks to be written.

## Why

Quad's Pure (The Party 1998, music by Darkxceed) and Theta (Wired 1998, 1st PC 64K, music by G-Day) synthesized all
but three or four samples with Darkxceed's "poespack". Their maps (`tools/xm-gen/maps/quad-*.json`) mark those samples
`poespack` with their generation slot only; the parameters and the engines are this change's.

## What Changes

- **Extraction**: `tools/xm-gen/extract.py` hooks poespack's voice, pad and chirp entry points and writes each
  generated sample's record (the voice's parameter block, the track's 16 steps of note, gate and slide, the
  instrument blocks), splitting `poespack` into functional engine ids (a wavetable phrase voice, an additive pad, a
  chirp, Pure's inline loop).
- **Engine types** for them; the phrase voice brings its own events and lanes (a sequencer unrolled along the
  trigger's path, `Engine::events` / `Engine::lanes`), not the one-note-per-run contract.
- **Per-sample parity** against the reference renders, then the song rows. Theta follows the 1998 compo executable:
  its instruments 4, 6, 7 and 8 differ from the later-build XM (recorded in the report).
- **Publication**: `quad-pure` and `quad-theta` songs (meta from `research/xm-softsynth-intros.md` §6), NOTICE,
  the listening-page group from add-fudge-softsynth, site rebuild.

## Discipline Skills

`doubt-driven-review`, `performance-optimization`, `systematic-debugging`.

## Capabilities

### Modified Capabilities
- `unified-synth`: the poespack engine types.
- `synth-convert`: phrase-engine events and lanes; the Quad maps' records; per-sample parity.
