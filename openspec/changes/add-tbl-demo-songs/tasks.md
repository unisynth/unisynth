## 1. Import

- [x] 1.1 Song tables: Wrap, Comb. Verify: `every_engine_effect_mode_has_a_table`.
- [x] 1.2 Same-sample on-before-off kept. Verify: `a_note_on_before_a_note_off_at_one_sample_keeps_its_order`; existing song suites.

## 2. Converter

- [x] 2.1 Echo bytes in range. Verify: `every_chain_value_is_a_parameter_in_range`.
- [x] 2.2 `--names`. Verify: `tracks_are_named_by_sample_record_and_class`.

## 3. Demo songs

- [x] 3.1 `make_demos.py`: three songs valid in `project/music/released/`; song/1 renders match the blob renders (median 0.1 dB per band and second). Verify: `song check`.
- [x] 3.2 NOTICE, unisynth.md, AGENTS rows.
