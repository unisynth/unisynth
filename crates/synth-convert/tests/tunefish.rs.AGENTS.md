# tests/tunefish.rs (sidecar: full detail of the AGENTS.md row)

Pull-only (not auto-injected). Tunefish conversion vs `tfsynth` legacy renders (loudness ≤ 1 dB, octave bands ≤ 3 dB via `common::metrics`).

## Always run
- Every `tfsynth` synthetic patch vs its legacy render (unison stacks averaged over repeats); `.tfm` structure/timing/master loudness; organ gate (loudness and end-of-note); parameter decode; pan; mod slots/targets; kind 32 in songs (inserts, effect automation, rejected routing, capacity); CLI.
- `an_enveloped_wavetable_is_bright_from_the_first_block`: ADSR→harmonics + ADSR→volume, first 50 ms bands 500 Hz–4 kHz within 4 dB of legacy.
- `a_tf4m_instrument_is_translated_to_the_final_layout`.
- `no_remorse_instruments_ring_as_long_as_their_legacy_render`: single note per no_remorse instrument, ring time vs tfsynth; skips without the synth-lab file.

## Opt-in env vars
- `TF_SONGS=<file|dir>`: every `.tfm` must convert and match its legacy render within 1 dB.
- `PARITY_DUMP=<dir>`: writes the legacy song of every synthetic patch for the parity tool.
- `NR_SOLO=1` (`no_remorse_solo_tails`): each instrument solo through `TfmPlayer` vs converted song; loud/quiet level Δ, tail end, per-instrument octave-band Δ.
- `NR_DUMP=<i>` (10 ms windows), `NR_WAV=<dir>` (solo `lane<i+1>-tunefish-reference.wav` / `-daw-unisynth.wav`), `NR_NOTE=<i>` (two short notes with/without effects; takes `NR_KEEP=<slots>`, `NR_NOMOD`, `NR_NOLP`, `NR_BANDS` (50 ms band table), `NR_QUIET`), `NR_DET=1` (unison and slop off).
