# src/main.rs (sidecar: full detail of the AGENTS.md row)

Pull-only (not auto-injected). CLI `synth-convert`; every command converts fully before writing the output file, warnings go to stderr, errors exit non-zero and write nothing. Text inputs are numbers with `#` comments.

## Commands
- `ws <in> <out>`; `tfm <in.tfm> <out> [tail-s]` (default 4); `tf4m <in.tf4m> <out> [tail]` and `tf4m-tfm <in.tf4m> <out.tfm>` (2013 Enigma Studio 4 songs); `patch <patch.txt> <phrase.txt> <out>`.
- `v2m <in.v2m> <out> [tail-s] [--channels hex] [--era auto|kkrieger2004]` → `v2conv::convert` (`Opts`).
- `xm-phrase <patches.json> <jizz|bluish|song0> <record> <variant> <out>` → `cauldron::phrase_song` at 44.1 kHz; prints `lead-frames N` on stderr (the parity suite drops them).
- `xm-triggers <in.xm>` prints the replay's triggers ("<start s> <volume> <offset> <channel> <instrument> <sample> <note>", `length` on stderr) for `cauldron.ts --onsets`.
- `xm <in.xm> <patches.json | map.json> <out>` (a JSON with `"format": "xm-gen/1"` → `xmgen::load` + `GenSource::new` (map checked against the XM, engines available) + `xm::convert::convert`; else the mini-cauldron path below; add-plain-xm-import). Mini-cauldron: `xm <in.xm> <patches.json> <out> [--names <tracks.json>] [--controls tick|row|note] [--thin <bytes>] [--keep-layers] [--split-pitches] [--dry] [--exact-chain]` → `import::convert_with` at 44.1 kHz. `--names` writes the track names as a JSON array (for `song import --meta`). stderr: `warning: …` lines, `tracks N phrase, M stored`, `layers N dropped (P same-tick, E echoes)`, `ambience N delay, M reverb tracks`, `clips N idle left out`, `pingpong-seconds`, `muted-seconds`, `lead-frames`.
- `migrate <in.song> <out.song>` → `SongM::read` + `write` (v3).
- `features <song> [--explain|--json] [--cost marginal.csv]`; `rust <song> <out.rs> [bank.dls]` → `rust::generate`.

See change: add-cauldron-xm-import, add-tbl-demo-songs, add-cauldron-control-rate, add-cauldron-lane-thinning, compact-cauldron-xm-import, slim-cauldron-chain, shrink-unified-synth, unify-v2-algorithms 6.5–6.8 (migrate).
- `elevated <music.asm> <out.song> [--table 7|8] [--solo k] [tail]` (Mentor's 4k synth: Elevated, Atrium, Receptor (carved by `tools/elevated-parity/carve.py`); reads `music.h` from the same directory; tail default 2 s; `--solo k` zeroes the other synths' volume like `tools/elevated-parity/render-ref.c`). See change: add-elevated-import.
- `e3prj <in.e3prj> <out.song> [--song id] [--editor | --engine 2011] [tail]` (Tunefish 3 project, player mode unless `--editor`; `--engine 2011` = `tf3::Mode::Early`, the 2011 engine of Pandora, add-tf3-pandora; `--engine` takes 2011 or 2012) and `tf3 <program.txt> <out.chunk> [--editor]` (one preset as a unified chunk). See change: add-tf3-import.
