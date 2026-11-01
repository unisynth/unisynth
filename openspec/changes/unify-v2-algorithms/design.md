# Design

## Context

See proposal.md for the motivation. Current state (after add-v2-import, commits up to `e1e1301`):

- `flt.rs` has `Kind::{SvfLp…Notch}` and, behind `us-flt-v2`, `Kind::V2(fmode)` that delegates to `v2::Flt` (LRC +
  Moog). The two SVFs are both Chamberlin state-variable filters, 2× oversampled:
  - WaveSabre: `low += f·band; high = q·(x − band) − low; band += f·high`, the first step fed `(x_prev + x)/2`, the
    output the average of both steps.
  - V2: `l += f·b; h = x − r·b − l; b += f·h`, both steps fed `x` (+ DC bias), the output the last step.
  The ladder and the V2 Moog are the same musicdsp 4-pole filter (`(in + prev)·p − k·state`, V2's `f = 1 − 2p = −k`).
  V2 runs it twice per sample, feeds back the clipped value and adds the bias.
- `env.rs`: `advance_ws` (closed-form linear/quadratic), `advance_tf` (Euler rate steps with power slope),
  `advance_v2` (closed-form per-frame add/multiply). The attack is linear in level in all three.
- `lfo.rs`: one phase accumulator; `shape` (f64 turns) and `shape_v2` (u32 counter, `utof23`).
- `osc.rs`/`voice.rs`: `Kind::V2` slots work on a mono running sum (Add/Mul/Set). `v2.voice` switches the output stage
  (amp = env 1, pan after distortion, DC filter, V2 transpose law). `era(p)` is read at 15 sites in the player.
- Fidelity checks: the parity reports (`tools/unisynth-parity/parity.ts`: WaveSabre/Tunefish device items + six
  songs; `v2.ts`: 15 + 3 V2 songs), with tolerances 1 dB loudness, 3 dB per octave band, onsets. Self-golden FNV hashes
  (`instrument/tests/golden.rs`, `fx/golden.rs`) pin the current numbers. `research/us-bake-check` checks that baked
  and blob builds agree.
- Names carry the source today: features `us-osc-v2`, `us-env-timed`/`-rate`/`-v2`, `us-fx-reverb-v2`…; parameters
  `v2.*`, `vdist.*`, `ch.*`; options "WaveSabre/Tunefish/V2", "V2 Moog low-pass", "V2 note"; modes DelayV2, ReverbV2,
  CompressorV2. Songs store parameters by name.

## Goals / Non-Goals

**Goals:**
- One algorithm per module, with V2/Tunefish/WaveSabre behaviour as settings any patch can set, named by function.
- The era branches leave the player: what a converter can express as a setting, it writes; what it cannot, and what a
  song audibly needs, is a quirk.
- Every converted song stays within its parity tolerance through every stage.

**Non-Goals:**
- No new sound design features beyond the union of the sides (no new filter topologies, no new LFO shapes).
- Effect algorithms stay as they are (renamed only).
- The DAW UI redesign for the new parameters: the parameter tables update; panel layout follows in add-synth-daw.

## Decisions

**D1. One formula per module, coefficients from settings (tolerance allows it).**
With fidelity judged by the parity tolerances, a module becomes one step with coefficients. For example, the
state-variable filter becomes `low += f·band; high = a·x − c·band − low; band += f·high`. The resonance form sets
`(a, c)`: input-scaled `(q, q)`, feedback-scaled `(1, r)`. The oversampling form sets the input blend and output
weights; the bias is a coefficient. The 4-pole filter becomes one step with the clip position and oversampling as
coefficients or a loop count. The envelope uses one stage machine whose curve is `v' = target + (v − target)·g(t)`:
quadratic, power or exponential, with the law turning parameters into `g`. The LFO uses one phase domain (u32 turns)
for every shape. Rounding drift is allowed and measured (D7).
*Alternative:* keep per-form step bodies for bit-identity. Rejected by the user (tolerance, not bit parity).

**D2. Settings, laws, quirks: three kinds of parameter.**
- *Settings* are musical and on every slot. Response, resonance form, oversampling form, feedback clip, curve, sustain
  slope, amplify, polarity, one-shot, phase, combine, amplitude source, pan stage and DC filter are settings.
- *Laws* map 0..1 to physical values: unisynth's perceptual laws (`hz`, `seconds`, `rate`) or 7-bit step laws
  (`step7`, value = step / 128, with the source's frequency curve). There is one law per module. Laws only touch
  control-rate code.
- *Quirks* reproduce particular source builds, one flag each, named by function. Candidates after D3:
  `boxed_osc` (4× box-averaged naive saw and pulse), `lcg_noise` (an old LCG noise), `integer_fm` (integer FM phase
  with a rounding collapse), `exact_math` (exact sine/arctangent instead of the fast approximations), `coarse_env`
  (early envelope laws on a 256-sample frame), `fixed_seed` (noise seeds at 0), `dc_bias` (the tiny DC offset added
  against denormals in filters and effects).
  **A candidate is kept only if the quirk survey shows a song leaving tolerance without it**; the others are deleted
  together with their code.
*Alternative:* keep `era` as one number. Rejected: an era hides which behaviour a song depends on.

**D3. Era branches that are only "off" or "0" move to the converter.**
The converter writes these directly:
- no FM before v4: generator off
- no aux before v6: generator off
- Moog bypass before v6: filter off
- no reverb low cut before v4: low cut 0
- no DC filters or bias before v6: off
- keysync 2 before v6: sync 1
The player loses those branches and the `era` parameter.

**D4. New layout, chunk versions 3/4, host migration.**
The parameter layout is redesigned by module: each filter slot, envelope, LFO and oscillator slot holds its settings
and law next to its fields. Instrument-level and quirk parameters go in the two-byte range. Version 3 has no speech
and version 4 has speech. The player rejects versions 1 and 2. `crates/song` migrates old chunks and old song-file
names on import, through a host-only table from the old kinds and modes to the new settings, so it costs no player
bytes. The exact index map is decided in task 1.2, and `doubt-driven-review` runs before it stands.

**D5. Features follow code, derived by `needs`.**
As in shrink-unified-synth, each setting value or law that adds code is a feature. `needs` derives the features from
the song, and a song pays only for what it sets.

**D6. No separate converted-song voice path.** `v2.voice` becomes voice settings: amplitude source, pan stage, output
DC filter. The oscillator law covers detune and colour, `voice.law` the transpose, and `voice.keysync` the note-on restart. The naive
generators stay oscillator kinds, and the combine op becomes a setting on every slot.

**D7. Gates per stage (any failure stops the stage).**
1. `cargo test --workspace`.
2. The parity reports pass within tolerance: all WaveSabre/Tunefish device items and the six songs (`parity.ts`), and
   the 18 V2 songs (`v2.ts`).
3. Drift against the previous stage is recorded per song (loudness Δ, worst band Δ), from `v2.ts`/`parity.ts` run
   against the previous stage's renders. It is not gated, but a song whose drift exceeds 0.5 dB loudness or 1.5 dB in
   a band is listed for the user to listen to.
4. Self-goldens are regenerated after gates 1–2 pass (`UPDATE_GOLDEN=1`), with the diff summarised in the stage
   results.
5. `research/us-bake-check/check.sh` ok (baked and blob builds still agree).
6. Size: the seven reference songs are not larger in squishy; xz is recorded. V2 songs are recorded.
7. Speed: the speed report stays within 5 %.

**D8. Functional names.** Names describe the function. Proposed (fixed in task 1.2 with D4):

| today | functional |
|---|---|
| osc type `V2`, `us-osc-v2` | naive generators: `naive saw/tri`, `naive pulse`, `sine`, `coloured noise`, `fm`, `aux in A/B`; `us-osc-naive` |
| filter `V2 *` / `V2 Moog *`, `us-flt-v2`, ladder | state-variable (forms `input`/`feedback`, oversampling `interpolated`/`held`), `4-pole`; `us-flt-feedback`, `us-flt-4pole` |
| `us-flt-route`, `v2.routing`, `v2.balance` | `us-flt-parallel`, `voice.routing`, `voice.balance` |
| env modes WaveSabre / Tunefish / V2, `us-env-timed/-rate/-v2` | laws `seconds` / `rate` / `frame`, curves `quadratic` / `power` / `exponential`; `us-env-seconds`, `us-env-rate`, `us-env-frame` |
| LFO mode V2, `us-lfo-v2` | settings on every LFO; law `frame`: `us-lfo-frame` |
| `v2.voice`, `us-v2-voice` | `voice.amp_source`, `voice.pan_stage`, `voice.dc_filter` |
| `vdist.*`, `us-vdist` | `drive.*`, `us-drive` |
| `ch.*`, `us-chan` | `strip.*`, `us-strip` (channel strip) |
| speech (Ronan), `us-speech` | `speech` (formant speech), unchanged |
| mod source `V2 note`, op `Add` | `note (centred)`, `Add` |
| fx ReverbV2 / DelayV2 / CompressorV2 / Cut | reverb `comb`, delay `modulated`, compressor `lookahead`, EQ `cut` |
| `us-v2`, `us-v2-any` | group `us-classic`? — replaced by the per-function features; the internal zero-size wrapper feature is named by what it gates (`us-mono-sum`) |

Source names remain in `crates/synth-convert` (input formats and the mapping), the parity tools, `research/` and a
provenance table in `docs/synth/unisynth.md`.

## D4 results: the new layout (task 1.2, revised after doubt-driven review)

Names: `crates/unisynth/src/params/rename.rs` maps every old index to its new name (`""` = removed). Names change where
the name was source-flavoured (`v2.*`, `vdist.*`, `ch.*`, `eg`) or where the meaning changes (`envN.mode` → `law`, the
mode is split into law + curve). Every other parameter keeps its song-file name. The target index map applies at the
format switch (task 6.1).

**Order of work (user decision 2026-10-31).** Stages 2–5 merge the engine code only. Each module's settings are an
internal struct, derived from today's type and mode values, so the stored parameters, the converters and the song
tables do not change, and each stage is gated on parity and drift. All new parameters, the converter writes and the
migration land once, at the format switch (tasks 6.5–6.8). Settings combinations that today's values cannot express
are tested on the engine API until then and as patches at the switch (6.5–6.8). *Alternatives:* format switch first, or interim
parameters at 260+ with an "as the type says" default; both rejected because they write converters and migration twice.

**Clocked oscillator.** The oscillator of change add-cauldron-xm-import (landed after 1.2) is renamed by function to
**clocked** (user decision 2026-10-31): it runs naive waveforms and a resonant filter at its own sample clock and
resamples to the song rate. Old → new: osc type `Cauldron` → `Clocked`, `us-osc-cauldron` → `us-osc-clocked`,
`OSC_CAULDRON` → `OSC_CLOCKED` (id 54 unchanged), `CD_*` → `CLK_*`, `Cauldron{Slot,T,F,Lp}` → `Clocked*`,
`osc::cauldron_*` → `osc::clocked_*`. The stored type value is unchanged, so no migration is needed. The source name
stays in the converter (`synth-convert xm`, `src/cauldron*`), the parity tool (`cauldron.ts`), research and the
provenance table, and "cauldron" joins the source-name ban. The oscillator adds no parameter indices: it reuses
`s0`–`s8` and `feedback`.

**Index map.** Old index → new index goes by name: the new index of `OLD[i].1`. A removed or split parameter has no
index of its own and is handled by the value rules below.

| block | new indices (v3) | parameters (new ones in bold) |
|---|---|---|
| voice | 0–15 | master, pan, capacity, steal, unison, uni_detune, uni_spread, mode, slide, glide, transpose, velocity, start_spread, one_shot, noise_level, env1_at_output |
| voice settings | 16–22 | **voice.amp_source** (full / envelope 1 only), **voice.pan_stage** (per slot / after drive), **voice.dc_filter** (off / on), **voice.law** (transpose: semitones / 7-bit steps), voice.keysync (free / restart / full reset; was v2.keysync), voice.routing, voice.balance (were v2.*) |
| osc 1–3 | 23–97, 25 each | 22 old fields (type, tracking, coarse, fine, octave, ratio, ratio_fine, fixed_hz, gain, mix, pan, pm_out, feedback, s0–s8), sample (moved in from 177–179), **combine**, **law** (unisynth / 7-bit steps). type = off, phase mod, saw/pulse, wavetable, sampler, **naive saw/tri, naive pulse, sine, coloured noise, fm, aux in A, aux in B**, clocked (Choice 13; came with change add-cauldron-xm-import, keeps its `s0`–`s8` and `feedback` meanings) |
| noise | 98–101 | band_on, hp, lp, width |
| filter 1–4 | 102–133, 8 each | type (off, **state-variable, 4-pole**, RBJ, resonator notch: Choice 5), **response** (low, band, high, notch, all-pass: Choice 5), cutoff, resonance, **form** (input / feedback resonance), **oversample** (state-variable: interpolated / held), **clip** (4-pole feedback before / after the clip), **steps** (4-pole: 1 / 2 per sample), **bias** (4-pole: off / on), **law** (Hz / 7-bit steps). The extra fields move the filter stride; 6.4 fixes the final indices |
| env 1–4 | 134–169, 9 each | law (seconds / rate / frame: Choice 3; was mode), attack, decay, sustain, release, **curve** (quadratic / power / exponential), curve_slope (power exponent; was slope), **sustain_slope**, amplify |
| LFO 1–2 | 170–185, 8 each | shape (sine, ramp up, ramp down, square, random, **triangle, sample-and-hold, pulse**: Choice 8), rate, depth, sync, law (Hz / frame; was mode), phase, polarity, one_shot (was eg) |
| mod 1–10 | 186–225 | source (…, `note (centred)` was "V2 note"), target, amount, operation |
| controllers | 226–228 | ctl.wheel, ctl.bend, ctl.bend_range |
| drive | 229–232 | drive.mode, gain, p1, p2 (were vdist.*) |
| controllers | 233–238 | ctl.cc2–cc7 |
| channel strip | 239 | strip.on (the strip DC filter is `strip.dc_filter`, two-byte) |
| mod 11–16 | 240–263 (two-byte) | as mod 1–10 (repo songs use mod 1–6, 9, 10; V2 patches need ≤ 8 at p95) |
| channel strip | 264–293 | strip.dc_filter, strip.volume, boost, order, send_reverb, send_delay, send_aux_a/b, recv_aux_a/b, drive_mode/gain/p1/p2, chorus_amount/feedback/delay_l/delay_r/rate/depth/phase, comp_mode/couple/auto_gain/lookahead/threshold/ratio/attack/release/out_gain (were ch.*) |
| speech | 294–296 | speech.on, speech.select (text and rate; was ch.speech_ctl4), speech.pitch (was ch.speech_ctl5) |
| quirks | 297–303 | **quirk.boxed_osc, quirk.lcg_noise, quirk.integer_fm, quirk.exact_math, quirk.coarse_env, quirk.fixed_seed, quirk.dc_bias** (candidates; D2 keeps only the needed ones) |

COUNT 304. Choices are plain `Choice(n)` in v3. The odd/even `Kinds` trick, which kept old stored values, is no longer
needed: the migration re-encodes values.

**Value rules (the migration, task 6.2; old value → new settings).** Value rules take precedence over the name map.
A choice is read with the old decoder (`kinds::decode` and its count), mapped by these rules to a new id, and written
as `inv_choice(id, n)` with the new count. A setting a rule does not mention takes its neutral default: sustain_slope
at the centre (constant), curve_slope at the old default, combine add, form, oversample and clip at the forms of the
unisynth kinds, laws Hz/seconds/unisynth.
- `filtN.type` splits into type + response + form + law:
  - SVF LP/HP/BP/notch → state-variable, input form, interpolated, Hz law
  - V2 LP/BP/HP/notch/all-pass → state-variable, feedback form, held, 7-bit law
  - ladder → 4-pole low-pass, feedback before the clip, 1 step, no bias, Hz law (`P4Form::ONE`)
  - V2 Moog LP/HP → 4-pole, feedback after the clip, 2 steps, bias, 7-bit law (`P4Form::TWO`)
  - RBJ HP/BP → RBJ + response; resonator notch → resonator
- `oscN.type` V2 + its old `s0` (mode 0–7: off, saw/tri, pulse, sine, noise, fm, aux A, aux B) → type off or the
  naive generator. `s2` (ring) → `combine` multiply. The colour stays in `s1`, so it keeps its mod targets. `s0` and
  `s2` become free for the naive types.
- `oscN.type` clocked (old id 6) → clocked; its shape fields and `feedback` move over unchanged.
- `lfoN.shape` is re-encoded from Choice(5) to Choice(8) for the unisynth law. A V2-law LFO's V2 shape (saw, tri, pulse,
  sine, S&H) maps to ramp up, triangle, pulse, sine, sample-and-hold (`lfo::FRAME_SHAPES`). Pulse is its own shape (a
  square starting low): mapping it to square with +½ turn of start phase would move a one-shot LFO's end (stage 3).
- `envN.mode` → law + curve: timed → seconds + quadratic; rate → rate + power; V2 → frame + exponential. The old
  `envN.slope` goes to `curve_slope` under the timed and rate modes, and to `sustain_slope` under V2 mode.
- `v2.voice` on → amp_source envelope 1 only, pan_stage after drive, voice law 7-bit (the oscillator law follows each slot's kind: 7-bit for the naive generators). Off → defaults.
- `v2.era` and the effect modes' `era` field → the era table below, applied to the migrated values. The converter uses
  the same function (`era_settings`), so the two cannot disagree. Both era fields leave the player.
- Chunk v1 becomes v3 and v2 becomes v4; the speech text section moves over unchanged. The chunk checks (`is_raw`,
  `is_structural`, sample indices) follow the new strides.
- **Automation lanes** follow the index map. A lane on a split choice (filter type, osc type, env mode, LFO shape) is
  re-encoded point by point with the same rules. A lane on a removed parameter (`v2.voice`, `v2.era`) is a migration
  error naming it; these are structural and no converter writes such lanes.
- **Mod targets:** positional targets 1–77 keep their fields. The colour stays in `s1`. The ids of
  `modmx::target::PARAMS` (78–141) and their order stay frozen. Their entries are named constants, so they follow the
  new indices; the entry for `envN.slope` (only V2 patches route to it) becomes `envN.sustain_slope`. Laws are chosen to
  reproduce the old mapping, so a route's physics is unchanged.

**Era table: every era branch of the player and where it goes.** Each row is a converter or migration write, or a
quirk candidate.

| branch today | where | becomes |
|---|---|---|
| aux generators off (era < 6) | `voice.rs` V2 slot | write: type off |
| FM generator off (era < 4) | `voice.rs` | write: type off |
| 4× box tri/saw and pulse (era < 3) | `osc.rs` | quirk `boxed_osc` |
| MSVC LCG noise (era < 5) | `osc.rs` | quirk `lcg_noise` |
| native sine/atan, integer FM (era < 6 and not the 2004 v5 build, id 7) | `osc.rs`, `lfo.rs`, `v2.rs` DistP, `chan.rs` | quirks `exact_math` and `integer_fm`; id 7 sets neither |
| 256-sample frame + year-2000 envelope laws (era < 2) | `env.rs` | quirk `coarse_env` (the song's event grid is the converter's, unchanged) |
| noise seeds 0 (era < 6) | `voice.rs` note-on | quirk `fixed_seed` |
| keysync 2 full reset only at era 6 | `voice.rs` note-on | write: keysync 2 → 1 below 6 |
| voice output DC filter (era ≥ 6) | `voice.rs` V2 out | write: `voice.dc_filter` on at 6 |
| Moog bypass (era < 6) | `flt.rs` | write: filter off |
| voice-source channel mods skipped (era < 5) | `instrument.rs` channel routes | write: those mod slots off |
| channel DC filters, bias (era ≥ 6) | `chan.rs` | write: strip DC filter setting (new strip field `strip.dc_filter`), quirk `dc_bias` |
| reverb low cut (era ≥ 4) | `fx/v2.rs` | write: low cut 0 below 4 |
| DC bias in effects (era ≥ 6) | `fx/v2.rs` | the effect mode's own `dc_bias` field (in place of `era`; effects are separate devices) |
| filter LRC bias | `v2.rs` | quirk `dc_bias` |
| program-change cut, CC7 reset (version < 4), speech routing (version ≥ 5), 256-frame event grid (version < 2) | `v2conv.rs` | stay in the converter (song events, not patch values) |

**Byte budget trade-off.** The one-byte range holds the fields converted songs set most: voice, slots, filters,
envelopes, LFOs, mod 1–10, drive, CC2–7 and the strip switch. The strip's other 29 fields cost one extra byte each
when set. Every converted V2 patch sets several of them, so the cost is one index byte per stored strip field. Task 6.4
measures the chunk bytes of the converted V2 songs and swaps the strip's most-set fields with CC2–7 if that is
smaller (mod 11–12 likewise against the reference songs).

**Review (doubt-driven, cross-model on @propose-review-1, two cycles).** Cycle 2 found: exact choice re-encoding,
the colour move contradicting the map, slope by name vs by value, automation lanes, the chunk checks, the effect-mode
era field, no era table, the keysync write, the 2004 build, the env-slope Add target, and strip fields set on every V2
patch. All are fixed above, and the strip/mod byte placement is measured at 6.4. Noise: law changes altering target
physics (the laws are chosen to reproduce the old mapping). Stopped after two cycles: the remaining items are
implementation detail owned by tasks 6.4–6.9. Cycle 1:
- Actionable and fixed above: there were no value rules for split and removed parameters; envelope `slope` maps by
  mode; chunk v3/v4 and the speech text were unspecified; there was no index rule; the PARAMS handling was unstated;
  the choice cardinalities were missing; mod 11–16 and drive/CC were placed badly.
- Trade-offs: new choice settings are not modulation targets. Renames change song keys, and the migration maps them.
- Noise: "sample has no index" and "osc block miscount". The sample is in the oscillator row (22 + sample + combine +
  law = 25).

## Stage results

References: the baseline at `28913c6` (`tools/unisynth-parity/unify-baseline.md`), HEAD `1ffe9f5` (cauldron and
`us-quiet` landed after the baseline). Renders and reports live under `~/Downloads/64k-tools/synth-lab/unify/`
(`base`, `drift-1ffe9f5`, `stage2`).

**Stage 1 (tasks 1.1–1.3).** `--drift` against the baseline renders on HEAD (functional names, old-name loading, and
the cauldron commits): 0.00 dB loudness and worst band on all 49 parity items and all 18 V2 songs.

**Speed reference.** HEAD renders elysian in 33.4 s against 24.7 s at the baseline (same output, interleaved runs of
both binaries). That slowdown came with the commits between the two, not with this change, so the speed gate compares
each stage with HEAD.

**Stage 2 (filters, tasks 2.1–2.3).**
- *Code:* one state-variable formula (`flt::sv_coefs`, forms input/feedback, interpolated/held, bias) and one 4-pole
  formula (`flt::p4_coefs`, feedback before/after the clip, 1×/2×, bias). The law gives only `f`; the clip form gives
  the resonance-to-feedback map, which keeps every combination stable. `Kind::V2` and the ladder kind are gone. The
  drive's filter modes run the same formulas (`v2::Flt`, `Moog`, `flt_coefs`, `Lrc::step_2x` removed). Without a
  feature that needs the forms (`us-flt-feedback`, `us-drive`, `us-strip`), the form code compiles to the plain
  input-form and one-step arithmetic.
- *Exactness:* the state-variable formula is exact for both earlier filters, and the 4-pole formula for the 2× filter.
  The ladder rounds `(in + prev)·p` where it had `prev·p + in·p`: within 1e-4 per sample of Tunefish's filter.
- *Gate 1:* workspace tests pass. bun: 170 pass, 1 fail (`shader-viewer` needs the `st` binary, absent in a fresh
  worktree; not this change). `check-unisynth-features.sh` passes.
- *Gate 2:* parity 49/49, V2 18/18.
- *Gate 3 (drift against the baseline):* every item below 0.005 dB loudness and worst band. Nothing to listen to.
- *Gate 4 (goldens):* four lines change, all through the ladder: `noise_band`, `filter_ladder_lp`,
  `filter_ladder_full_resonance`, `filter_series_lp_hp`. Peaks are unchanged. `golden_fx.txt` is unchanged.
- *Gate 5:* bake check 7/7.
- *Gate 6 (size against HEAD, `songs-size.sh`):* squishy is unchanged on all seven reference songs and all six V2
  songs. xz moves −32 to +120 B on the reference songs and −164 to +172 B on the V2 songs.
  - A first version cost +16 to +148 B xz and two squishy steps on the reference songs, and +212 to +576 B on the V2
    songs: the form code was built everywhere, and the drive kept its own filter copy. Gating the form code (`FORMS`)
    and sharing the filter with the drive fixed both.
- *Gate 7 (speed against HEAD, interleaved, median of 3):* elysian +0.5 %, engage +0.7 %, eladamri −0.2 %,
  no-remorse +3.4 %, tf4testsong +1.1 %. V2 songs are not slower (the parity reports' × real time is equal or higher).
  elysian's output is byte-identical to HEAD.
- *Spec corrections:*
  - The 4-pole high-pass is the input minus the low-pass, as in V2. Its rejection a decade below the corner is about
    10 dB, so "Shared 4-pole filter → High-pass on the Hz law" now asks for 6 dB, not 24.
  - "Forms combine freely" is checked at zero resonance, because the all-pass response (low + band + high) peaks with
    resonance.
- *Found for stage 3:* the frame-law pulse is low in the first half-turn, the inverse of the Hz-law square, so the
  value rule "pulse → square" needs +½ turn on the start phase.

**Stage 3 (LFOs, tasks 3.1–3.2).**
- *Code:* one LFO for every law (`lfo.rs`).
  - `LfoP` reads the settings per field (`shape_of`, `rate_of`, `one_shot_of`, `view_of`, `sync_of`), so each step
    reads only what it uses.
  - Shapes are one union of eight: sine, ramp up, ramp down, square, random, triangle, sample-and-hold, pulse.
  - Rate in turns per second from the Hz or frame law; one-shot; polarity as Add-view `gain`/`dc`; start phase; key
    sync restart / song time / continue.
  - One `advance` (one-shot hold, sample-and-hold redraw on wrap) and one `value`. The frame law's sine is the table
    sine: the fast sine and the native sine of formats below 6 are gone from the LFO. `V2P`, `advance_v2`, `shape_v2`
    and `sync_v2` are removed; copies stay in the tests.
- *Settings on every LFO:* `lfoN.polarity`, `lfoN.one_shot` and `lfoN.phase` now apply under both laws. They default to
  neutral, so no song changes.
- *Size work:* a first version read every setting at every call site, and its unused `powf` calls stayed in the code
  (+150 to +260 B xz on the LFO songs, two squishy steps). Reading per field and keeping the earlier arithmetic where
  `us-lfo-frame` is off fixed it.
- *Gate 1:* workspace tests and feature builds pass.
- *Gate 2:* parity 49/49, V2 18/18.
- *Gate 3 (drift against stage 2):* one song moves: fr-052_platinum, worst band −0.03 dB at 8 kHz (the table sine).
  Nothing to listen to.
- *Gate 4 (goldens):* unchanged.
- *Gate 5:* bake check 7/7.
- *Gate 6 (size against stage 2):* squishy is unchanged on all 13 songs. xz moves −28 to +44 B on the reference songs.
  The V2 songs are −512 B raw, −244 to +116 B xz.
- *Gate 7 (speed against HEAD, interleaved, median of 3):* elysian −1.0 %, engage +1.2 %, eladamri −2.1 %, no-remorse
  +3.3 %, tf4testsong +2.0 %. V2 songs, × real time against stage 2: median +1.2 % (single runs, −8.4 % to +6.7 %).
- *Design correction:* the frame law's pulse stays its own shape, so the D4 shape choice has 8 values.

**Stage 4 (envelopes, tasks 4.1–4.2).**
- *Code:* one stage machine (`env.rs`).
  - `EnvP` has `law` (seconds / rate / frame), `curve` (quadratic / power / exponential, base 2), `curve_slope`,
    `sustain_slope`, `amp`, `coarse` (the early frame laws: quirk candidate `coarse_env`) and `instant`.
  - The law gives each stage a time constant, computed only in the stage that runs. The attack is linear from 0, or
    from the current level on the frame law; the curve shapes decay and release.
  - The sustain slope and the amplify apply to every law: the seconds law can now have a sloped sustain.
  - Each curve keeps its end rule. `advance_ws/_tf/_v2` and `trigger_v2` are removed; copies stay in the tests.
  - `check_v2` → `check_frame`.
- *Exactness:* the seconds law is exact, and only `env_tunefish` changes among the goldens (rounding).
- *Size work:* first version +400 to +616 B xz and squishy steps. Fixes:
  - each curve's branch const-gated on its law's feature (a build with one law drops the others' code; `powf` had
    stayed in seconds-only builds);
  - a base-2 exponential (no `expf`/`ln`);
  - lazy per-stage times;
  - `Env::advance` out of line.
- *Gate 1:* workspace tests and feature builds pass.
- *Gate 2:* parity 49/49, V2 18/18.
- *Gate 3 (drift against stage 3):* one song moves: fr019, worst band −0.02 dB at 8 kHz. Nothing to listen to.
- *Gate 4 (goldens):* `env_tunefish` only, peak unchanged.
- *Gate 5:* bake check 7/7.
- *Gate 6 (size against stage 3):* squishy is unchanged on all 13 songs. The reference songs are −68 to 0 B xz; the V2
  songs (frame law only) +44 to +208 B xz.
- *Gate 7 (speed, interleaved, median of 5, against stage 3):* no-remorse +0.3 %, tf4testsong +1.4 %, eladamri
  −0.3 %, kkrieger +0.6 %, brullwurfel +1.7 %. Cumulative against HEAD: about +3.6 % on the rate-law songs (stage 3
  +3.3 %). Single interleaved runs against HEAD scatter ±6 % on this machine.

**Stage 5 (combine and voice settings, tasks 5.1–5.3).**
- *Code:*
  - Every slot meets the running sum through one step, `osc::mix_in`: add (panned), multiply (ring) or replace (both
    channels).
  - `osc::sample` returns unpanned output, and the voice pans. The naive generators return a plain value; their
    Add/Mul/Set is the slot's `combine`, set at control rate (`V2Out` removed).
  - One `slot_targets` for every kind, with the 7-bit oscillator law for the naive generators (`v2_slot_targets`
    removed).
  - `VoiceSet` (amplitude source, pan stage, DC filter, transpose law) replaces the scattered `v2.voice` reads.
  - `params::era_settings` is the only reader of `voice.era`: settings the converter will write, and the quirk
    candidates. `the_format_version_is_read_in_one_place` walks `src/` and fails on any other read (checked to fail
    closed).
  - The running mono sum is unpanned for every slot; before, an unisynth slot added its panned mean. That only matters
    for a naive generator after a unisynth slot.
- *Deferred to the format switch (decision C):* the `oscN.combine` parameter and the quirk parameters with their
  features. The "Quirk disabled" scenario comes with them (6.1, 7.1).
- *Spec correction:* "Ring on a band-limited slot". The saw's third and fifth harmonics times the carrier land on
  110 Hz, so "no 110 Hz" cannot hold. The scenario now asks that the carrier not pass on its own, and that 110 Hz
  hold only those products.
- *Gate 1:* workspace tests and feature builds pass.
- *Gate 2:* parity 49/49, V2 18/18.
- *Gate 3:* no item drifts.
- *Gate 4:* goldens unchanged.
- *Gate 5:* bake check 7/7.
- *Gate 6 (size against stage 4):* squishy is unchanged on all 13 songs. xz moves −120 to +108 B on the reference
  songs and −92 to +196 B on the V2 songs.
- *Gate 7 (speed against stage 4, median of 3):* −2.3 % to +1.6 %, except no-remorse at +4.3 %. On no-remorse, 5
  interleaved rounds against HEAD: stage 4 +3.0 %, stage 5 +3.4 %.

### Song inventory (decision 2026-11-01)

The player may reject chunk versions 1 and 2, but no song we hold may be left on them: each is migrated (host
migration, tasks 6.5–6.6) or re-converted from its source (tasks 6.7, 6.9). Counts as of 2026-11-01.

| Location | Songs | Kind | Brought over by |
|---|---|---|---|
| `project/music/*.song.json` | 8 (demo + 7 reference) | tracked song/1 | migration, resaved |
| `project/music/patches/*.patch.json` | 2 | tracked patches | migration, resaved |
| `project/music/released/*.song.json` | 28 (25 WaveSabre intros, 3 TBL) | tracked song/1 (main `cf1c106`) | migration, resaved |
| `project/music/released/v2/*.song.json` | 347 V2 songs | tracked song/1 (main `a721861`) | migration, resaved (re-conversion from the external corpus: optional cross-check) |
| DAW templates and fixtures (`packages/daw/src/app.js`, `main.ts`, `test/library.test.ts`, `test/editing.test.ts`, `bench/dense.ts`) | embedded | code | edited to the new names and values |
| `research/us-size-probe/out/songs/*.song`, `legacy/` | 28 + 16 | git-ignored binaries | re-converted (`measure.sh`, `songs-size.sh`) |
| `project/music/.library.json` | 1 | git-ignored, generated | regenerated (`synth-convert` library example) |
| `research/tbl-mini-cauldron/out` | 3 XM songs + phrases | generated | re-converted (`synth-convert xm`, `xm-phrase`) |
| `synth-lab/ws` | 108 WaveSabre `.bin` | external source | re-converted per parity run |
| `synth-lab/tunefish/media/tf4modules` | 2 `.tfm` | external source | re-converted per parity run |
| `synth-lab/v2songs` `acceptance`, `fr_public`, `modland`, `carved` | 15 + 20 + 325 + 1 `.v2m` | external source | re-converted |
| `synth-lab/v2songs/parity/*.song` | 70 | cached conversions | regenerated (stale caches deleted first) |
| User files outside the repo (the DAW's browser storage holds only UI preferences) | unknown | user-held | migration on load and in the DAW on open (6.6) |

The load-all check (task 6.9) walks every tracked location (external and git-ignored ones when present, reported) in the table except the last row and fails when a song does not
load in the new player, is skipped, or a location holds fewer songs than recorded.

## Format-switch review (doubt-driven, 2026-11-01)

Three adversarial reviewers (one same-model, two cross-model) read tasks 6–7, D1–D7, D4 and the inventory against
the contract "every song migrates, sound within tolerance, old versions rejected". Findings and what changed:

- **No layout marker in song files (blocker).** `song/1` stores named values with no version, and the v3 choice
  re-encoding keeps names such as `envN.law` (rate law 1.0 today, frame law under `Choice(3)`). A migrated file looks
  like an un-migrated one. Song files become `song/2` / `us-patch/2`; only `/1` migrates (task 6.6).
- **Clocked.** Its stored value is an expanded-choice position (3/8). Under plain `Choice(13)` it re-encodes like every
  choice. The "no migration needed" note above holds only for its fields (`s0`–`s8`, feedback).
- **Missing work.** Converters, `Patch`, `auto.rs`, `features` (6.7); the frozen old layout with `era_settings`
  host-side (6.5); the DAW migrating on open (6.6); merging `main`, which holds the released songs (6.1).
- **Value rules.** FM and aux-in generators combine by replace, ring by multiply (stage 5 code). The 7-bit oscillator
  law follows the slot kind, not `voice.stage`. Raw-encoded polarity, one-shot, sync, routing and keysync, and the Cut
  EQ's DC filter (on from format 6) get rules (6.4). Effect chunks have no version: the new effect modes take new ids,
  so old ones are rejected rather than misread.
- **Lanes.** One old lane becomes one lane per changed setting. Corpus check over the 383 tracked songs: no lane on
  any type, law, shape, voice or raw choice; none automates a law and its slope; the 94 `oscN.sN` lanes are on
  clocked and unisynth slots, none on a naive slot's mode or ring. V2 never modulates mode or ring (V2 parameters
  2/3, 8/9, 14/15 are not destinations). Such lanes and routes are therefore a named migration error, not a design case.
- **Order.** The quirk survey runs before the freeze (6.3), and stored-pair counts per song are checked before the
  layout is fixed (6.4), so neither forces a second layout change.
- **Drift.** The 0.5/1.5 dB thresholds flag songs for listening (D7.3, unchanged); baseline renders of every tracked
  song (6.2) give each one a drift report.
- **Trade-off kept.** The trusted (baked) chunk path checks no version and assumes well-formed input: baked songs are
  generated from the same tree. The no-panic and rejection requirements apply to the checked build.
- **Rejected.** Two findings (cauldron names and `era(` reads still in the player) came from a stale read; both are
  gone (`88cd0a3`, stage 5).

### Cycle 2 (same date)

- **One switch commit.** The migration, `song/2`, the converters and the player's v3 reading cannot be verified
  apart: each needs a player that reads v3. Tasks 6.5–6.8 are checkpoints of one commit; the workspace is green
  before and after it.
- **4-pole forms.** The value rules above had the clip placement swapped; they now match `P4Form::ONE` (ladder) and
  `P4Form::TWO`. The filter block stores `steps` and `bias` as well as `clip`.
- **Envelope law and slope both automated.** Supported rather than rejected (spec "Automated envelope mode"): the
  slope lane splits into `curve_slope` and `sustain_slope` by the law in force at each point.
- **Quirk survey.** Candidates are split first (`integer_fm` from `exact_math`; DC bias from the DC filters). A quirk is
  kept when clearing it moves a song past the drift thresholds against the quirk-on render, or past the parity
  tolerance where an original render exists. The integer-FM scenario follows the survey's result.
- **Size.** Contract: no squishy growth for songs that don't use the new settings. Defaults are chosen so that
  reference songs store no new pairs: `curve` defaults to "follows law" (quadratic for seconds, power for rate,
  exponential for frame), filter `response` to the type's own. The freeze is gated on a squishy probe of the reference
  songs built with a prototype of the layout, and the switch commit is gated again (D7.6, D7.7).
- **Effect chunks** get a version marker in v3, so an old effect chunk is rejected rather than decoded as another mode.
  The Cut EQ's DC filter (on from format 6) is an effect setting next to `dc_bias`.
- **Tracked songs migrate** (`song/1 → song/2`); re-converting from the external corpora is an optional cross-check.
  A `/2` file with an old parameter name fails to load (names are migrated only with `/1`).
- **Names.** The DAW's user-facing strings join the source-name check; song credits ("a Farbrausch V2 song") are
  attribution, not names, and are exempt. Merging `main` applies the 1.3 and clocked renames to its new code.
- **Checked player API.** `LoadError` gains `UnknownVersion`; `is_channel` and the other index-order checks follow the
  new strides.

## Risks / Trade-offs

- [Drift accumulates across stages until a song leaves tolerance] → D7.3 records drift per stage. A song near the
  edge after a stage is examined before the next one.
- [Tolerance passes but a song sounds different] → songs above the D7.3 thresholds are listed for the user to listen
  to, and the change ends with an A/B listen before archive.
- [Deleting a quirk loses a song only a listener would notice] → the survey reports the drift for every song with the
  flag cleared, and deletions are listed in the stage results.
- [Renames ripple into song files, the DAW and the converters] → the migration maps old names, and the rename
  happens once (task 1.3). Coordinate with add-synth-daw, which edits the parameter tables.
- [The per-segment coefficient path slows the inner loop] → coefficients are computed at control rate; gate D7.7.

## Migration Plan

1. Record the baseline: parity reports, the renders (WAV) of every parity item as the drift reference, self-goldens,
   bake check, size and speed.
2. Fix the layout and the names (tasks 1.2–1.3).
3. Do the stages in order: filters, LFOs, envelopes, oscillator combine plus voice settings. Each stage passes D7.
4. Then the format switch (tasks 6.1–6.9, order revised by the format-switch review): merge `main`, baseline renders
   of every tracked song, quirk survey, freeze the layout with a pair-count check, the frozen old layout and the
   migration, `song/2`, the converters, and only then the player rejects versions 1/2.
5. Rollback: each stage is its own commit. Old songs are unaffected until the format switch.
