## MODIFIED Requirements

### Requirement: Mini-cauldron XM import
`synth-convert xm <in.xm> <patches.json> <out.song> [options]` (`--names`, `--controls`, `--thin`) SHALL convert an XM made by mini-cauldron (TBL Jizz, Stash song 1, Stash BLUISH), given that intro's decoded patch data, into a unified song that holds no generated sample data and whose every effect parameter lies in 0..1. `--names` SHALL write the track names (master excluded) as a JSON array; `--controls` SHALL set how often XM volume, pan and pitch reach the song (default `tick`).

#### Scenario: Jizz converts
- **WHEN** the Jizz XM and `patches_jizz.json` are converted
- **THEN** the converter writes a song that loads and renders in `unisynth`, and that song holds no sample data for any `Probe` sample

#### Scenario: Converted songs pass the song check
- **WHEN** a converted song is imported with `song import` and its track names
- **THEN** `song check` reports it valid

#### Scenario: Track names
- **WHEN** an XM is converted with `--names`
- **THEN** a phrase track is named `<sample> (record r)` or `<sample> (record r, ±class)`, a stored one `<sample> (sample)`, `.smp` dropped and repeats numbered

#### Scenario: Control rate
- **WHEN** an XM is converted with `--controls row` or `--controls note`
- **THEN** each trigger's volume, pan and pitch change only at row starts, or hold their first tick's values, and the song is smaller than with `tick`

#### Scenario: Thinned lanes
- **WHEN** an XM is converted with `--thin 4`
- **THEN** every thinned lane stays within 4 bytes of the per-tick steps at every frame, a jump of more than 4 bytes stays an exact step, and the song has fewer automation points than without
