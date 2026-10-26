# Design

## Where the bytes are (measured, 2026-10-26)

Probe: i686 `no_std`, opt-level z, core rebuilt with `optimize_for_size`, `panic=immediate-abort`, squishy 0.1.3.

**Function groups** (`attr.sh`, no LTO, no inlining; ranks, not totals). no_remorse, 64 KB of code:

| Group | Bytes | Note |
|---|---:|---|
| `unisynth` | 26.8 K | `Voice` 9.5 K, `UInstrument` 1.7 K, `wt::Table` 1.5 K, `Matrix` 1.4 K, reverb 1.3 K, `SlotRamps` 1.2 K, filter slot 1.1 K, env 0.9 K |
| `libm` | 11.8 K | `rem_pio2(_large)` 3.3 K, `pow` 2.1 K, `powf` 1.7 K, `sqrt` 0.9 K, `sincosf`, `expf`, `log10f`, `floor`, `exp2f` |
| `synth_core` | 10.7 K | `mix::{player, song, mixer, registry}` 7.9 K, `VoicePool` 1.2 K, `Pool` 1.2 K |
| trait impls (`_`) | 9.8 K | `run`, `note_on`, `new`, `next` of the unified types |

The legacy Tunefish probe has 10.2 K of `libm` too (same functions), so `libm` is not why unified is bigger, but it is the
largest block both pay. The legacy `.tfm` path plays through `TfmPlayer` (no song parser, no mixer): about 8 K of the
Tunefish gap is `synth-mix`, which the legacy WaveSabre probes pay as well (they play through it).

**Module cost** (`marginal.sh`, all modules against one removed, raw / xz bytes): `us-sample-gsm` 6.1 K / 2.8 K, `us-fx-eq-parametric`
5.1 / 1.5, `us-fx-reverb-rooms` 4.6 / 1.4, `us-osc-wt` 4.1 / 2.2, `us-fx-delay-tempo` 3.6 / 1.1, `us-fx-chorus` 3.1 / 1.3,
`us-fx-flanger-phaser` 3.1 / 1.0, `us-fx-reverb-freeverb` 3.1 / 0.6, `us-sample-dls` 2.6 / 1.4, the other effects 0.5–2.6 K raw.
Before step 1 `us-osc-pm`, `us-osc-blit`, `us-noise` and the four filter families cost 0: their code was linked
regardless. After it each saves 0.5–1 K raw (0.1–0.4 K xz).

## D1. A module feature removes the module's code

A feature used to be checked in `Kind::check` / `Kind::from_type` (the load fails) while the `match` that runs the module
stayed ungated, so the compiler could not know the arm was dead. Arms now read `Kind::X if cfg!(feature = "us-…")` with a
bypass arm, which the compiler folds. The Wavetable arm already carried a `#[cfg]` on its variant. Tests that call a
module's functions directly are gated on its feature. No behaviour changes for a build that has the feature.

## D2. The map carries reasons and cost

`synth-convert features` stays a flat comma list (the build uses it). `--explain` prints one line per feature: the
devices, parameters and automation lanes that require it, and its marginal cost from `out/marginal.csv` when present.
`--json` prints the same for tools. The analysis already walks these facts (`needs.rs`); it returns them instead of
dropping them.

## D3. Candidates, in the order measured value / effort suggests

Each is a task with a gate: it lands only if a reference song packs smaller by more than the 512-byte squishy step and
parity, goldens (or documented moves) and the feature gate hold.

1. **`libm`.** The laws need `pow`, `exp2`, `log10`, `sin`, `cos` on bounded arguments. `rem_pio2(_large)` is only for
   large arguments: sine of a phase already in [0, 1) turns needs none. A small set in `unisynth::fm` (not `synth-core::math`, which the legacy engines share) (range
   reduction plus a polynomial, or `exp2`/`log2` pair with `pow = exp2(y·log2 x)`) could replace ~8 K raw. Cost: values
   move in the last bits; goldens regenerate; parity must hold. The legacy engines would profit equally, so the gain
   shows in the comparison only if applied to both (it is: `synth-core` is shared).
2. **Voice switches.** One `cfg!` per path that a song may not use: unison > 1, mono/slide/glide, LFOs, controllers
   (wheel, bend), one-shot, start-spread, fine grid, steal. Measure each by stubbing it out in an ablation build before
   adding a feature; keep only those above ~0.5 K raw.
3. **Matrix sources and targets.** The matrix queries every target through `Tab`; a song that routes five of 50 targets
   pays for all. Gate by feature groups of targets (pitch, osc gain, filter, noise, env, LFO) if the ablation shows ≥ 1 K.
4. **Baked patches.** A generated `Spec` (const per song: slot kinds, used matrix routes, unison count) with `Voice<S>`
   generic over it. This is the largest possible gain and the largest refactor; the DAW needs the runtime path as well.
   Decide only after 1–3, from what is left; run `doubt-driven-review` first.

## Results

- Step 1 (guards): −0.5 to −2 KB packed per song.
- D3.1 (`fm.rs`): `libm` 11.8 KB → 0.3 KB in the no_remorse player; packed (squishy, compo configuration), legacy → unified before → after
  both steps: Global Groove 14,336 → 20,992 → 16,384 (+14 %), Elysian 43,520 → 49,152 → 45,568 (+5 %), Engage 36,352 → 42,496 → 38,400 (+6 %),
  Eladamri 30,208 → 34,816 → 31,232 (+3 %), no_remorse 17,920 → 29,184 → 23,552 (+31 %), tf4testsong 17,408 → 27,648 → 22,016 (+26 %).
  The legacy engines still carry their ~10 KB of `libm`; the same replacement would shrink them too.
- What is left for the Tunefish songs (about 5.6 KB over legacy): `synth-mix` (the legacy `.tfm` player has no song parser or mixer, about 8 KB raw),
  the generic voice and matrix. D3.2–3.4 decide from here.

## Risks

- A guard hides a code path from a feature set the tests do not cover: `tools/check-unisynth-features.sh` builds and
  tests every single-module set and `tools/check-song-features.sh --minimal` proves each listed feature is needed.
- Dropping `libm` precision changes sound: bounded by the 1 dB / 3 dB parity gate and the self-goldens, which are
  regenerated only with the parity run in the same change.
