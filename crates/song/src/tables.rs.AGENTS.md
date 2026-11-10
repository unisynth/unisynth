# crates/song/src/tables.rs (sidecar: full detail of the AGENTS.md row)

Pull-only (not auto-injected).

## API
- `Param { index, name, group, default, law (unisynth `Law`), unit, min, max, options, structural, raw }` (`physical(p)`, `range()`); `Table { kind, mode, label, params }` (`param(name)`).
- `table(kind, mode)`, `tables()` (instrument first, then `EFFECT_MODES`), `of_device`, `mode_of`, `kind_name`, `mode_names`, `EFFECT_MODES`.

## Instrument (kind 64)
- Built from `unisynth::params` (`name`, `law_of`, `DEFAULTS`, `is_structural`, `is_raw`); group = name prefix before the first dot (else `voice`).
- `choice_options`: options per field — osc type 25 positions (id = stored position: `Off`, `Points`, `Phase mod`, `PAD table`, `Saw/pulse`, … `Clocked` at 24; "—" at free positions, add-tf3-import), point-field segment modes `Step/Linear/Spline`, combine, law, filter type 11, form, oversample, law, env law (`Seconds/Rate/Frame`) / curve, LFO shape 8 / sync / law / polarity, mod source 17 (`Note (centred)`) / operation 3, voice settings; mod targets (`Law::Index`) named from `unisynth::modmx::target`. `Law::Steps` unit `steps` 0..128; raw (sampler index) range 0..65535.

## Effects (kinds 65–73)
- `mode` (structural `Choice(fx::mode_total)`, unused positions `—`) + parameter names in legacy order with editor defaults (legacy constructor values via law inverses; TF EQ 0.5 = unity). Exported chunks always carry their values.
- Modes: Distortion Shaper / Curve (`x1 y1 x2 y2 power1..3 dry_wet`) / Power / Wrap (`drive` 0.25 = ×1, `offset` 0.5 = 0); EQ Parametric / 3-band / Cut (7-bit, `dc_filter`); Delay Tempo / Ms / Modulated; Flanger Phaser / Ring / Comb (`delay amount sweep restart`; defaults pass through); Reverb Freeverb / Compact / Rooms / Comb; Compressor Compressor / Look-ahead.
- Label `<Kind> · <mode>` when the kind has several modes.

See change: add-v2-import, add-tbl-demo-songs, add-cauldron-xm-import, unify-v2-algorithms.
- Osc type labels "Shaper tone" (7), "Sweep drum" (9); 7 free positions left. See change: add-fudge-softsynth.
