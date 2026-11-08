# src/tfm.rs (sidecar: full detail of the AGENTS.md row)

Pull-only (not auto-injected). Tunefish `.tfm` / `.tf4m` → unified song.

- `convert_tfm(blob, tail)`: track per non-empty instrument (instrument + inserts); master last with receives ×11 and volume 200/32768. `Clock` replays the legacy player's `f32` 512-frame block clock to place events at block starts; note 0 dropped, velocity 0 = off, note & 0x7f.
- `convert_patch(patch, phrase, tempo, length)`: one instrument + `(block, note, velocity)` phrase.
- `tf4m_to_tfm(bytes)` → (final-layout `.tfm`, warnings): 2013 `.tf4m` has `TF4M_COUNT` 110 values (drops PROFILE, MODULATIONTYPE, RANDOM, SUBOSC, SHAPE, TEST; index −6 from noise on; wheel/BP/notch 0), mod source `round(4p)`→`/14`, target `round(16p)` via `TF4M_TARGETS`→`/39` (PAN→none), amount `((p−½)·20)²`→final law, reverb width 1. Warns on LFO→VOL/HPCUT (2013 sine −0.5…1.5), GenMod>0 (2013 sine table), MM9/10. Refuses a final `.tfm`.
- `convert_tf4m(blob, tail)` = `tf4m_to_tfm` + `convert_tfm`.
