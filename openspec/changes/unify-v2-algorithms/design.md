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
format switch (task 6.1). Until then each stage adds its new parameters after the current ones.

**Index map.** Old index → new index goes by name: the new index of `OLD[i].1`. A removed or split parameter has no
index of its own and is handled by the value rules below.

| block | new indices (v3) | parameters (new ones in bold) |
|---|---|---|
| voice | 0–15 | master, pan, capacity, steal, unison, uni_detune, uni_spread, mode, slide, glide, transpose, velocity, start_spread, one_shot, noise_level, env1_at_output |
| voice settings | 16–22 | **voice.amp_source** (full / envelope 1 only), **voice.pan_stage** (per slot / after drive), **voice.dc_filter** (off / on), **voice.law** (transpose: semitones / 7-bit steps), voice.keysync (free / restart / full reset; was v2.keysync), voice.routing, voice.balance (were v2.*) |
| osc 1–3 | 23–97, 25 each | 22 old fields (type, tracking, coarse, fine, octave, ratio, ratio_fine, fixed_hz, gain, mix, pan, pm_out, feedback, s0–s8), sample (moved in from 177–179), **combine**, **law** (unisynth / 7-bit steps). type = off, phase mod, saw/pulse, wavetable, sampler, **naive saw/tri, naive pulse, sine, coloured noise, fm, aux in A, aux in B** (Choice 12) |
| noise | 98–101 | band_on, hp, lp, width |
| filter 1–4 | 102–133, 8 each | type (off, **state-variable, 4-pole**, RBJ, resonator notch: Choice 5), **response** (low, band, high, notch, all-pass: Choice 5), cutoff, resonance, **form** (input / feedback resonance), **oversample** (interpolated / held), **clip** (4-pole feedback before / after the clip), **law** (Hz / 7-bit steps) |
| env 1–4 | 134–169, 9 each | law (seconds / rate / frame: Choice 3; was mode), attack, decay, sustain, release, **curve** (quadratic / power / exponential), curve_slope (power exponent; was slope), **sustain_slope**, amplify |
| LFO 1–2 | 170–185, 8 each | shape (sine, ramp up, ramp down, square, random, **triangle, sample-and-hold**: Choice 7), rate, depth, sync, law (Hz / frame; was mode), phase, polarity, one_shot (was eg) |
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
  - ladder → 4-pole low-pass, clip after, 1×, Hz law
  - V2 Moog LP/HP → 4-pole, clip before, 2×, 7-bit law
  - RBJ HP/BP → RBJ + response; resonator notch → resonator
- `oscN.type` V2 + its old `s0` (mode 0–7: off, saw/tri, pulse, sine, noise, fm, aux A, aux B) → type off or the
  naive generator. `s2` (ring) → `combine` multiply. The colour stays in `s1`, so it keeps its mod targets. `s0` and
  `s2` become free for the naive types.
- `lfoN.shape` is re-encoded from Choice(5) to Choice(7) for the unisynth law. A V2-law LFO's V2 shape (saw, tri, pulse,
  sine, S&H) maps to ramp up, triangle, square, sine, sample-and-hold.
- `envN.mode` → law + curve: timed → seconds + quadratic; rate → rate + power; V2 → frame + exponential. The old
  `envN.slope` goes to `curve_slope` under the timed and rate modes, and to `sustain_slope` under V2 mode.
- `v2.voice` on → amp_source envelope 1 only, pan_stage after drive, oscillator and voice laws 7-bit. Off → defaults.
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
when set. Every converted V2 patch sets several of them, so the cost is one index byte per stored strip field. Task 6.3
measures the chunk bytes of the converted V2 songs and swaps the strip's most-set fields with CC2–7 if that is
smaller (mod 11–12 likewise against the reference songs).

**Review (doubt-driven, cross-model on @propose-review-1, two cycles).** Cycle 2 found: exact choice re-encoding,
the colour move contradicting the map, slope by name vs by value, automation lanes, the chunk checks, the effect-mode
era field, no era table, the keysync write, the 2004 build, the env-slope Add target, and strip fields set on every V2
patch. All are fixed above, and the strip/mod byte placement is measured at 6.3. Noise: law changes altering target
physics (the laws are chosen to reproduce the old mapping). Stopped after two cycles: the remaining items are
implementation detail owned by tasks 6.1–6.3. Cycle 1:
- Actionable and fixed above: there were no value rules for split and removed parameters; envelope `slope` maps by
  mode; chunk v3/v4 and the speech text were unspecified; there was no index rule; the PARAMS handling was unstated;
  the choice cardinalities were missing; mod 11–16 and drive/CC were placed badly.
- Trade-offs: new choice settings are not modulation targets. Renames change song keys, and the migration maps them.
- Noise: "sample has no index" and "osc block miscount". The sample is in the oscillator row (22 + sample + combine +
  law = 25).

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
4. Then the format switch to chunk version 3/4, the migration, the repo songs, and the quirk survey (deleting quirks
   no song needs).
5. Rollback: each stage is its own commit. Old songs are unaffected until the format switch.
