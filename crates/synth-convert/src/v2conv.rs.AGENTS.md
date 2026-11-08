# src/v2conv.rs (sidecar: full detail of the AGENTS.md row)

Pull-only (not auto-injected). V2 → unified song. See change: add-v2-import, unify-v2-algorithms 6.5–6.8 (the format switch).

- `convert(bytes, Opts)` → `Converted` (parse + `song_opts`); `RATE` 44100; `FRAME` 128.
- `patch(p, version, &mut warnings)` / `patch_with(p, version, Profile, &mut warnings)` → `[f32; COUNT]`: V2 raw/128 laws, mod entries → Add slots (16 kept by amount, warning), era value (`Profile::{Auto, V5_2004}`, CLI `--era auto|kkrieger2004`).
- `song` / `song_opts(&V2m, Opts{tail, mask, profile}, &mut warnings)` → `SongM`: per-(channel, program) tracks, events on the 128-frame (v<2: 256) grid, program-change cut + CC1–6 reset (v<4: CC7=127), CC1–7 + speech automation, channel-16 Ronan text (v≥5), reverb/delay buses, master EQ cut + compressor v2.
- Aux A/B: each reader (receive or aux osc) receives straight from the lower-MIDI-channel senders' aux output pairs (V2 frame order; a level is used when > 0 or Add-routed); no aux bus track (5.2); wiring reads the migrated v3 patches.
- Builds in `params::old` (old source/op encodings, era); `SongM::write` migrates to v3.
