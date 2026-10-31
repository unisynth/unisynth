## MODIFIED Requirements

### Requirement: Mini-cauldron XM import
`synth-convert xm <in.xm> <patches.json> <out.song> [--names <tracks.json>]` SHALL convert an XM made by mini-cauldron (TBL Jizz, Stash song 1, Stash BLUISH), given that intro's decoded patch data, into a unified song that holds no generated sample data and whose every effect parameter lies in 0..1. With `--names` it SHALL write one name per track (master excluded) as a JSON array: `<sample> (record r)` or `<sample> (record r, ±class)` for a phrase track, `<sample> (sample)` for a stored one, repeats numbered.

#### Scenario: Jizz converts
- **WHEN** the Jizz XM and `patches_jizz.json` are converted
- **THEN** the converter writes a song that loads and renders in `unisynth`, and that song holds no sample data for any `Probe` sample

#### Scenario: Converted songs pass the song check
- **WHEN** a converted song is imported with `song import` and its track names
- **THEN** `song check` reports it valid
