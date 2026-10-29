# Instrument and preset inventory for the unified synth (2026-10-26)

What instruments and presets exist locally that the DAW (`openspec/changes/archive/2026-10-30-add-synth-daw`) can offer in its library,
where they live, their licence, and how each becomes a `unisynth` patch. Counts come from
`openspec/changes/archive/2026-10-30-add-synth-daw/mockup/gen-catalog.py`, which writes `catalog.json` (names and counts only) for the
mockup. The lab files are never committed.

## Summary

| Source | Location (lab default) | Count | Licence | Becomes a unisynth patch by |
|---|---|---:|---|---|
| GM.DLS bank (Windows) | `~/Downloads/64k-tools/gmdls/gm.dls` | 235 instruments (128 GM bank 0, 98 variations, 9 drum kits), 495 waves | Microsoft OS bank, read at runtime, never shipped | sampler slot source (wave today; instrument with key regions proposed) |
| GM bank (macOS) | `CoreAudio.component/.../gs_instruments.dls` | 235 instruments / 495 waves (8-bit) | Apple OS bank | same; names and waves differ from Windows (`research/softsynth-research.md` §GM.DLS) |
| Tunefish 4 programs | `synth-lab/tunefish/patches/tf4programs/program*.txt` | 92 named programs | GPL-3 repo: reference only, re-save as our own patch | `synth-convert patch` (Tunefish 112-value layout → slots, design D10 of `add-unified-synth`) |
| Tunefish songs | `synth-lab/tunefish/media/tf4modules/*.tfm` | 2 (`no_remorse`, `tf4testsong`) | GPL-3 repo | `synth-convert tfm` (instruments + insert chains) |
| WaveSabre reference songs | `synth-lab/ws/refs/*.bin` (converted from 52 `.als`/`.xrns`/`.flp` in `WaveSabre/Tests/WaveSabreConvertTests/Tests`) | 52 songs, 915 instrument instances: Slaughter 480, Falcon 227, Thunder 175, Adultery 29, Specimen 4 | WaveSabre repo test songs (MIT) | `synth-convert ws` per device |
| Repo test patches | `crates/wavesabre/tests/songs.rs`, `crates/tfsynth/tests/patches.rs` | 7 WaveSabre instrument tests + 15 tfsynth patches | ours (MIT) | `synth-convert` (same paths as above) |
| 64klang | `synth-lab/64klang` | no preset files in the lab (player source only) | MIT | not supported by `unisynth` |
| Project presets | `project/music/patches/*.patch.json` | 0 today (the 6 in the mockup are placeholders) | ours | native `us-patch/1` |

## Notes

- **WaveSabre instance counts are not unique sounds.** Songs reuse patches across tracks and the test songs repeat each
  other. A real library should dedupe by chunk bytes and take names from the source projects (`.als` is gzip XML,
  `.xrns` is zip XML; both hold track names). The blobs have no names, so the catalog names them `song · kind n`.
- **Adultery picks GM drum-kit waves.** In the 10 songs that use it, Adultery plays single waves, mostly from the
  "Standard" and "TR-808" kits (waves 193, 18, 220, 272, 435, 192 …) plus a few one-off effects ("Reverse Cym.",
  "Train", "Bird") and "Syn.Strings" waves. That is the case one wave per sampler handles well.
- **Melodic GM instruments are key-split.** Only 51 of 235 instruments have a single region; "Piano 1" has 10 waves
  over the keyboard and the drum kits up to 62. `unisynth`'s sampler (`add-unified-synth`, sampler spec) plays **one
  wave** chosen by index, so a melodic GM instrument would stretch one wave over all keys. Playing GM instruments
  properly needs a **DLS-instrument source**: pick (bank, program, drums), choose the region per note with
  `dls::Instrument::region_for(key, vel)` (already in `crates/dls`) and apply its `wsmp` root key, tuning and loop.
  It fits the existing sampler "source" choice parameter, so the chunk layout stays v1 if the index field holds
  the instrument index. This belongs in `add-unified-synth`, not the DAW.
- **The bank differs per OS.** A GM-based intro sounds different on Windows and macOS (names and waves differ), and
  Linux has no system bank. Pick per source: GSM samples in the chunk are portable; GM.DLS is free in bytes but
  platform-dependent.
- **Tunefish programs** are text files: the first line is the name, then `Param;value` lines in the Tunefish layout. They
  are GPL-3 data from the Tunefish repo, so the DAW should treat them as references to convert and re-save, not ship
  them as-is (same policy as `research/softsynth-research.md`).

## Regenerate

```bash
python3 openspec/changes/archive/2026-10-30-add-synth-daw/mockup/gen-catalog.py   # needs the lab + cargo (dls dump example)
```
