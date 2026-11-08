# tests/cauldron.rs (sidecar: full detail of the AGENTS.md row)

Pull-only (not auto-injected). Integration tests of the mini-cauldron converter. Patch data from `research/tbl-mini-cauldron/ref/`; opt-in tests skip (stderr `skip: …`) when `research/tbl-mini-cauldron/out` (made by `run_all.sh`) is absent. See change: add-cauldron-xm-import, compact-cauldron-xm-import.

- `every_record_loads_and_plays_within_full_scale`: every record (Jizz, BLUISH, song0) at ratios 1 and 2: `instrument` → chunk → loads and renders in the unified player, audible, peak ≤ full scale (the clip).
- `every_record_with_its_effect_chain_loads_and_plays`: instrument + `effects` devices on one track load and play, peak ≤ 4 (comb doubles full scale; the original's 8-bit clamps are dropped).
- `every_phrase_song_loads_and_plays`: all phrase songs (32 Jizz + 53 Stash variants) load and play after their lead-in.
- `the_tbl_xms_read` (opt-in): the three rebuilt XMs parse; Jizz 14 ch / 17 patterns / 82 instruments / speed 6 / bpm 151; `Probe11.smp` decodes to `render_jizz/Probe11.s8`.
- `the_tbl_songs_replay` (opt-in): the three songs replay with no unsupported effect and end 0.1 s before openmpt's render (its tail), to the frame.
- `the_tbl_songs_unroll` (opt-in): every phrase trigger unrolls with on/off alternating and events inside the trigger; a native trigger has ratio 1; groups tracks (count = per-(record, class) max concurrency); prints triggers, notes, ping-pong seconds, semitone classes, tracks, pairs, widest bend, bend warnings.
- `the_tbl_stored_samples_play` (opt-in): every stored sample renders C-4 at ≥ 0.95 correlation with its decoded GSM; Stash stored = drum kit (count, lengths); prints GSM bytes, GSM factor `×up` and codec correlation at the XM rate.
- `the_xm_cli_converts_and_writes_nothing_on_error` (opt-in): CLI on Jizz. `--keep-layers --split-pitches` = 58 phrase / 13 stored, 0 layers dropped; default = 13 phrase / 11 stored, 591 layers dropped, smaller file, loads, 11 samples; default has reverb devices and an `ambience` line, `--dry` none, same tracks, fewer delays. `with_tremolo` patches pattern 0's first effect to 7 → error naming effect, pattern, row, channel, no output.
