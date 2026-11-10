# src/module.rs (sidecar: full detail of the AGENTS.md row)

Pull-only (not auto-injected).

- Module ids for `UnsupportedModule` (never renumbered;
- 1–13 osc/sample/noise/filter/env, 16–30 `FX_*` per mode, 31–35 `VOICE_*`, 36 `MATRIX`, 37 `AUTOMATION`, 38 curve, 39 `MATRIX_ADD`, 40 `SPEECH`, 41–52 functional (`ENV_FRAME`, `OSC_NAIVE`, `FILTER_FEEDBACK`, `FILTER_PARALLEL`, `DRIVE`, `LFO_FRAME`, `VOICE_OUT`, `STRIP`, 4 V2-mode fx), 53 `CHUNK_WIDE`, 54 `OSC_CLOCKED`, 55–56 wrap/comb, 57–61 quirks, 62 `OSC_COMBINE`).
- One `modules!` table: id consts, `ALL`, `const fn feature(id)`, `compiled(id)` (`cfg!` per feature), `compiled_all(ids)` (generated songs' compile-time assert), `quirk_module(q)`, host `name(id)`.

## History

- See change: shrink-unified-synth, add-v2-import, add-cauldron-xm-import, unify-v2-algorithms 1.3, 6.5–6.8
- `FX_CHORUS_TF3 = 65` "us-fx-chorus-tf3" (chorus, one-tap mode; add-tf3-import 6.4).
- 66 `OSC_PAIR`, 67 `VOICE_DROP`, 68 `ENV_LINEAR`, 69 `FX_EQ_SVF_LFO`, 70–72 `FX_DISTORTION_ATAN/SINE/KNEE`, 73–74 `FX_DELAY_PINGPONG/ALLPASS`. See change: add-elevated-import.
- 75 `us-osc-shaper-tone`, 76 `us-osc-sweep-drum`. See change: add-fudge-softsynth.
