# Design

## Context

See proposal.md (Why). Source read for behaviour: `enigmastudio/Enigma-Studio-3` `code/eshared/synth`
(`tunefish3.hpp`, `tf_oscillator.cpp`, `tf_addsynth.cpp`, `tf_instrument.cpp`, `tf_modmatrix.cpp`, `tf_player.cpp`,
`tf_song.cpp`, `tf_ieffect.cpp`). No licence is stated, so we treat it as all rights reserved: we read it for
behaviour, write a functional spec, and implement from that spec. This is the process `add-tf-synth` used for
Tunefish 4 (GPL).

Facts that shape the approach (confirmed in `research/tunefish3-functional-spec.md`, task 1.1, which is the
authority where this list is shorter):

- Fixed 44.1 kHz, stereo, **384-frame** blocks. Control (mod matrix, spline rebuild, filter coefficients) runs once
  per block. Output is `i16` at `5000 · volume`.
- 129 parameters, stored as bytes over a per-parameter value range (`TF_PARAM_VALUERANGE`, `/100` by default). In
  `.e3prj` they are floats keyed by name (`<param value=".." name="OscP1Off"/>`).
- Voice: point oscillator + PAD table + noise, summed. The oscillator and PAD sum runs at `k·44.1 kHz` (k = 1..8 from
  the note frequency, k = 1 below 689 Hz), **always** passes two cascaded 12.83 kHz ladder low-passes (also at k = 1)
  and is averaged down. Then LP, HP, BP and notch run
  in series, each switched on above 0.5. After that come the 10 insert slots.
- Point oscillator: the **spline object is per instrument, not per voice**. `update` runs in each voice's process
  with that voice's matrix state, so the cycle a voice hears depends on the voice processed before it. The table is
  257 samples (natural cubic spline through all points; step/linear/spline per segment), read without interpolation. Sub-oscillator: up to 2 passes, the second at `freq·(0.5 ∓ detune)` and
  half volume. Unison: up to 11 layers at `f ± k·spread`. Drive is `32p³ + 1`, then hard clip, then a linear L/R
  balance with a volume ramp over the block.
- PAD table: 2^16 samples used (2^17 allocated) per instrument, profiles single/detune/spread/gauss, `freq/260` phase
  increment, which sounds **an octave below the note** (×0.499). No unison, no pan, **no velocity** (a released PAD
  voice without a volume envelope drones until stolen). Drive is `32p + 1`, then hard clip.
- Mod matrix: 10 slots, `M(t) = ∏ A·src` with a symmetric squared amount `A = (20(p − 0.5))²` (0..100). Sources:
  LFO1/2, ADSR1/2, MOD1–4 (per voice, set by a note's effect word: high byte = slot 1–4, low byte/128 = value),
  wheel (never fed). LFO sine is `sin φ + 0.5` and the ramps reach 0..2, so LFOs can raise a target.
  Targets: point offsets/values, volume, freq, pan, detune, spread, drive, noise, every filter cutoff/res/Q, and both
  ADSR decays.
- Player build (`ePLAYER`) compiles out the band-pass and notch filters, the MOD1–4 inputs, and the chorus,
  flanger and EQ effects. **The released intros played their songs without these**, even when the project sets them.
- Song: `bpm`, patterns (rows × tracks of `{note, instrument, velocity, effect}`; velocity −1 plays 128; a note with
  instrument −1 is ignored; note 128 is a note-off), pattern instances `{rowOffset, seqTrack 0–15, patternId}`. The
  player rounds `t·bpm/15` to the row, so rows fire **half a row early**, one row per 384-frame block. Master ×11
  × 5000 into i16. Instrument parameters are quantised to the stored byte (per-parameter range) in the intro.

Current `unisynth` state: the oscillator type is `Choice(13)` stored as an f32. Shape values 0–8 per slot are
matrix targets (ids 4–48), and envelope decay targets (60–63) exist. The wavetable slot (`Wt`) already plays a
per-voice cycle with layers, L/R detune, spread, drive and hard clip; the Tunefish 4 converter (`tf.rs`) maps
filters, envelopes, LFOs, matrix and effects. Parameters ≥ 240 need `us-chunk-wide`.

## Goals / Non-Goals

**Goals:**
- Transplant and the per-module synthetic songs pass the shared parity metrics against the reference renderer.
- A song that uses neither new type renders byte-identically to its golden; the new code is feature-gated.
- Reuse the Tunefish 4 mapping wherever Tunefish 3 has the same law, and add nothing that duplicates it.

**Non-Goals:**
- Bit-exactness, or reproducing the shared-spline cross-voice leakage exactly (see D3).
- Extracting Pandora and Train Control from their executables (follow-up).
- A Tunefish 3 legacy crate. Like V2, Tunefish 3 converts straight into `unisynth`; the reference is external.

## Decisions

### D1. Functional spec first, then code

Task 1 writes `research/tunefish3-functional-spec.md` in the structure of the Tunefish 4 spec (overview, signal
flow, modules, effects, parameter table with laws, song format and timing, quirks, open questions). Each section
gets a "differs from TF4" note. The spec ends with a **feature survey**: which modules, interpolation modes, PAD
profiles, matrix targets and effects Transplant and the 128 presets actually use. Survey results can narrow
D3–D6 (for example, if no song modulates points, the point oscillator still supports it, but parity cannot test
it).

### D2. Reference renderer from the external checkout, in player configuration

`tools/tf3-parity/build-ref.sh` clones Enigma-Studio-3 at a pinned commit into `$TF3_REF` (default `/tmp/tf3-ref`)
and compiles `code/eshared/synth/*.cpp` with our MIT shim header. The shim supplies `eF32`/`eU32`/`eArray`,
`eRandom*`, the scalar `eF32x2` ops, `eMemSet` and `eDataStream`. We add our own `render-ref.cpp`, which parses
`.e3prj` with a minimal XML reader, sets instruments by parameter name, drives `tfPlayer` block by block and writes
a WAV. It builds with `-DePLAYER` by default (what the intro played) and `-DeEDITOR`-style (all modules) on request.
Alternative considered: running `estudio3_s.exe` under Wine and capturing audio. We rejected it because Wine audio
capture is fragile, it needs a GPU, and it renders the editor's configuration, not the player's.

### D2a. YouTube captures as a second, coarse reference

Public captures of the released intros exist (checked 2026-11-09, audio fetched with `uvx yt-dlp -x`, 44.1 kHz s16 WAV):

| Intro | Video id (Annikras HQ capture) | Length | Integrated | Music starts |
|---|---|---:|---:|---:|
| Transplant (2011) | `31cDnVWhS7w` | 220 s | −13.0 LUFS | 1.4 s |
| Pandora (2011) | `RlyIJD9AEWM` | 252 s | −17.5 LUFS | 7.5 s |
| Train Control (2012) | `Gm0NWMdcbmU` | 161 s | −14.9 LUFS | 0.9 s |

(Alternatives: `HX1r2si67h8`/`JDLCligopn8` Transplant, `Ncsm5NL2QcA`/`2aaTvQvZDrI` Pandora, `g9FBiARP4jI` Train Control.)

`tools/tf3-parity/captures.sh` fetches them into `$TF3_CAPTURES` (default `~/Downloads/64k-tools/tf3-captures`).
They are only a coarse reference:
- lossy codec (Opus/AAC) with an unknown capture gain and chain, so loudness is normalised and not compared, and bands
  are compared only from 63 Hz to 12.5 kHz;
- an unknown start offset and possible drift or dropped frames, so the alignment uses the shared onset
  cross-correlation (`alignment`), per 30 s window, and the report lists lag and correlation per window; timing is
  judged by that correlation, not by peak-picked onsets (on the capture the picker found 11 hits against 25 in the
  render);
- the whole-song mix only (no per-instrument check).

Result (task 1.4): the Transplant reference render in player mode matches the capture within 0.20 dB in every octave
band, at a constant lag (768 frames after the first-sound estimate, i.e. a 1.403 s capture offset) with correlation
0.71–0.89 in all 7 windows; editor mode differs by up to 1.30 dB. Onset curves are computed once per whole signal:
slicing per window puts a false onset at each cut, which biased an earlier version to lag 0. The capture is a
usable reference, and the intro is player mode.

Uses: (1) decide the player vs editor question for Transplant by listening and comparing band balance with and
without the editor-only modules; (2) the acceptance check for Pandora and Train Control once their songs are
extracted (follow-up); (3) a fallback if D2's build fails. In that case Transplant's parity runs against the capture
with the coarse tolerances (bands 4 dB, onsets within 2 blocks, match ≥ 0.9), and the synthetic per-module items are
checked against the functional spec's unit tests instead.

### D3. Point oscillator = a new cycle builder on the wavetable playback

The type is `points`, feature `us-osc-points`. Playback reuses `Wt`'s per-voice cycle, layer loop
(`|inc ± det ± k·spread|`), non-interpolated read, drive and hard clip. Only the cycle builder differs:

- Points `(0,0)`, then 1–4 user points, then `(1,0)`. Each user offset is placed in the remaining span
  (`x = last + (1 − last)·clamp(px, 0.01, 0.99)`), values are `2p − 1`, and each segment's interpolation is step,
  linear or spline. The spline is one natural cubic through all points (functional spec §3.1.1).
- 257 samples (256 + guard), mean removed; rebuilt at a control segment when a point input changed, or always when a
  segment uses spline mode (the source re-solves every block; equal output).
- **Per voice**, not per instrument. This is a deliberate difference from TF3's shared spline. It only matters
  when voices modulate points differently; it is recorded as a known difference and checked by parity.

Storage (frozen by task 2.1, see D5):
- `shape[0..8]` = P1–P4 offset and value (so matrix targets 4+15i+6…13 move them), `shape[8]` = drive.
- Appended per-slot block `pts` (3 slots × 9, two-byte indices 294..321, `params::COUNT` 321): point count
  `count(1..4)`, five segment modes `Choice(3)` (segments ending at points 1–4, then the last), layers `count(1..11)`,
  layer detune `10p³` Hz and layer spread `10p⁴` Hz (the wavetable's laws). A `pad` slot reads the layer detune. All
  default 0, so builds without `us-chunk-wide` (which zero indices ≥ 240) lose nothing.
- **Sub-oscillator**: the converter emits a second `points` slot at octave −1, gain ½ and the same point values,
  with the matrix point routes duplicated to it. TF3 clips sum-then-drive and we clip per slot; this is a known
  difference when the sub-oscillator and drive > 1 are combined.
- **Oversampling and decimation filter** (functional spec §3.3, revised after task 1.1): TF3 renders the oscillator
  and PAD at `k·sr`, `k` the smallest integer with `k·sr ≥ 64·f` (≤ 8), then **always** runs two cascaded ladder
  low-passes at 12.83 kHz (designed at `k·sr`) and averages k samples. `points` and `pad` slots therefore always run
  this whole stage, k included (task 3.2: no switch, as these types exist for Tunefish 3 songs). It is per slot, the
  slot gain applied before it (TF3 filters its volume-scaled signal), the envelope after it. Cost (task 3.2,
  `pts::tests::speed_of_the_stage`, release build): one voice with 11 layers runs at 82× real time at ×1 and 11× at
  ×8 (a 6 kHz note); notes below 689 Hz never oversample
  (TF3 filters the oscillator + PAD sum; the ladder's output saturation `y − y³/6` makes this a small known difference).

### D4. PAD oscillator = static shared table, structural parameters

The type is `pad`, feature `us-osc-pad`. `shape[0..5]` = bandwidth, bandwidth scale, damp, harmonics, profile
(`Choice(4)`: single, detune, spread, gauss), drive. They are **structural**: the table is built once at load into
the pool (2^16 f32 = 256 KB per slot and instrument, not per voice; TF3 allocates 2^17 but writes and reads 2^16). `need` counts it once, and `set_param` ignores the structural values. The
converter rejects automation of them with an error naming the parameter, consistent with "Fixed routing".
Playback follows TF3: increment `f/260` table periods per sample over 2^16 samples (the fundamental sounds at
0.499·f, an octave low; the converter keeps TF3's octave, the slot reproduces the factor), non-interpolated, L/R
detune, drive and hard clip, no pan, **no velocity** (the converter adds no velocity route to a `pad` slot). The
table is computed in place in its own 2^16 floats (task 3.3): the real part of an inverse DFT is the inverse DFT of the
spectrum's Hermitian part, whose samples come out of one 2^15-point complex FFT (even samples in the real parts, odd in
the imaginary ones), with bin values computed when needed and phases from a counter hash of an instrument-rng seed,
so no scratch memory and no change to the wavetable's FFT. Checked against the direct inverse DFT, and against the
original table's magnitude spectrum per profile (`pad::parity`, opt-in `pad-ref`, bands of 16 bins within 2 % of the
peak band; phases do not change it). Load time 5.7–17 ms per table (release). The per-voice state (two phases, the
decimation stage) is a `Pts` from the pool; the table is shared read-only by the voices. Random phases come from the instrument rng, so the table differs from TF3's but has the
same spectrum.

### D5. Type choice positions (frozen by task 2.1)

The oscillator-type choice grows from `Choice(13)` to `Choice(25)` and **the id is the stored position**: every
earlier type `k` moves to id `2k`, whose stored value `2k/24` has the same f32 bits as the old `k/12`, so every
existing chunk decodes to the same type and renders byte-identically (goldens, effect goldens and the bake check
unchanged). The new types take odd positions, `POINTS = 1` and `PAD = 3`; the other odd positions (5..23) are free
and a chunk naming one is `BadChunk`, in the loader (`Kind::from_type`) and in `needs` alike. Call sites that use
named constants with `osc_type::COUNT` keep working; the three that used numeric relations were rewritten
(`is_naive`: even ids `NAIVE_SAW..=AUX_B`; `naive_mode`: `(t − NAIVE_SAW)/2 + 1`; V2 aux: `[AUX_A, AUX_B][k]`).
Module ids 63 `us-osc-points` and 64 `us-osc-pad` exist from task 2.1 (the loader reports them until the slot kinds
land in 3.1 and 3.3). The song tables give the choice 25 labels ("—" at free positions, the label count always equals
the positions); the DAW maps labels to its own kind index (Points 13, PAD table 14), hides "—" options and counts only
real options for its lazy-select threshold.

**Doubt-driven review (task 2.1, cross-model on `@propose-review-1`, 2 cycles).** Actionable findings, all fixed:
gap ids fell into the loader's catch-all (`UnsupportedModule(SAMPLER)`) and `needs` accepted them; no module ids or
features existed for the new types; a packed `Choice(243)` segment-mode word would be unusable in the DAW (replaced by
five `Choice(3)` fields); the DAW hard-codes kind 4 for the sampler (kept valid by mapping labels, not positions);
the 25-entry choice crossed the DAW's lazy-option threshold; the V2 aux-B path that the fix makes load-bearing had no
test (added). Accepted trade-offs: a stored type value that is not an exact position (no tool writes one; the type is
structural and never automated) can decode differently than before; shape 7 is structural for every type (sampler
source), so a point slot's point-4 value is not automatable (the mod matrix still moves it; Tunefish 3 has no
automation); `naive_mode` only debug-asserts its input (gaps are refused at load before it runs).

### D6. Instrument mapping (`tf3.rs`)

Mapping goes through physical values, as for the other converters.

| Tunefish 3 | Unified |
|---|---|
| point oscillator (`Osc*`) | slot 1 `points` (+ slot 2 `points` sub-oscillator when `OscSubOsc` ≥ 0.5) |
| `Add*` | slot 3 (or 2) `pad`; `AddVolume` 0 → no slot |
| `OscVol`/`AddVolume`, `OscPan` | slot gains, equal-power pan with TF4's √2 compensation |
| `OscPoly`, `OscGlide`, `OscSlop` | capacity, glide (block constant 384), warning like TF4's `GenSlop` |
| noise, LP/HP/BP/notch | as TF4 (`tf.rs`), laws re-checked; BP/notch dropped in player mode |
| ADSR 1/2, LFO 1/2 | as TF4, laws re-checked (block 384 instead of 512 for rate-law envelopes) |
| matrix 1–10 | Scale slots (amount `(20(p − 0.5))²` through the Scale amount law); point targets → shape targets; ADSR decay → 60/61; MOD1–4 → controllers CC2–CC5 (dropped in player mode); wheel dropped (never fed) |
| LFOs | TF3 shapes reach outside 0..1 (sine `sin φ + 0.5`, ramps 0..2): needs a matching LFO shape/offset in `unisynth` or is a known difference (decided in task 4.2) |
| parameters | quantised to the stored byte (§5 range column) in player mode before mapping |
| effects 1–10 | as TF4's insert mapping (kinds 65–73, Tunefish modes); gain → gain effect; chorus/flanger/EQ dropped in player mode |

`--editor` converts with every module the editor had; the default is player mode. Each module that player mode
drops produces a warning.

### D7. Song mapping

Each instrument used becomes one track (the instrument, then its inserts), with a master track last, as for `.tfm`.
Pattern instances are expanded in row order. Row r fires at the start of the first 384-frame block whose start time
rounds to row r (`fi(t·bpm/15)`, round half to even: half a row early), and only one row fires per block. Velocity −1
plays 128; a note with instrument −1 is ignored (the editor's "previous event" lookup is UI only). A new note on a
sequence track's column releases that column's previous note by note number, and note 128 does the same. Effect words become a controller event on the instrument's track just before the note. This is
per instrument where TF3 is per voice, a known difference for overlapping notes. Master: instruments ×11, volume
`5000/32768` so the unified 16-bit output is the player's.

### D8. Parity and material

`tools/unisynth-parity` gets a `tf3` group: synthetic `.e3prj` songs written by a test helper (one instrument and a
phrase per module: point interpolation modes, sub-oscillator, unison, PAD profiles, filters, envelopes, matrix
targets, each effect), plus Transplant from `$TF3_REF/demos/transplant`. Metrics are the shared ones; onsets are
within 384 frames. Songs, renders and the checkout stay outside the repo; the report (metrics only) is committed.

Result (task 5.1, `tools/unisynth-parity/tf3-report.md`): 29 synthetic items and Transplant pass. Transplant: loudness
0.06 dB, judged bands within 0.93 dB, timing lag 0 (correlation 0.81); against the capture within 1.13 dB. Root causes
found on the way (systematic debugging): a fixed-frequency oscillator's frequency route acts squared (`(p·M)²`, now two
Scale routes); the PAD table's partial widths and bin membership follow the original's f32 operations (tables now match
its magnitude spectrum to 0.00 % per 16-bin band); unison and PAD items vary per run in the original itself (random
start and table phases from the tick count), so unison and the sub-oscillator play 32 notes, PAD items 16 long notes,
and PAD items are accepted within the original's own measured variance (up to 1.9 dB loudness and 3.7 dB per band
between tick seeds).

## Risks / Trade-offs

- [The ES3 source may not build outside MSVC] → The shim is limited to the synth directory; if a file resists, it
  is stubbed and the gap is listed in the spec. If the build fails, fall back to the captures (D2a) and ask before
  relaxing tolerances further.
- [Captures are lossy and of unknown gain] → Used only with normalised loudness, a capped band range and windowed
  alignment (D2a); never the sole gate when the reference build works.
- [Per-voice vs shared spline, per-slot vs summed drive, per-instrument mod inputs] → Documented known
  differences, measured by dedicated parity items. Exact emulation is a follow-up only if a song fails.
- [PAD memory: 256 KB per PAD instrument] → It is pool memory sized by `need`. Transplant's count goes in the
  memory table; the songs are for a 64k intro whose RAM is not the limit, but load time is measured.
- [Appended parameters and type positions are frozen once songs exist] → `doubt-driven-review` before task 3
  builds on them.
- [Licensing] → Behaviour-only reading, our own code and tables, no source in the repo. The music stays local until
  Brain Control agrees.

## Migration Plan

Additive. Existing chunks and songs keep their meaning (D5, goldens), and no migration is needed. Rollback means
removing the features; the converter refuses `.e3prj` without them.

## Open Questions

- Exact multiplicative matrix rule and the `TF_PARAM_VALUERANGE` table (functional spec).
- ~~Does Transplant use the editor-only modules?~~ Yes (an inert LFO → BP route and an EQ); the capture shows the
  released intro is player mode (D2a), which is the converter's default.
- Oversampling vs band-limited cycle for `points` (decided by parity, D3).
- Should the 128 presets go into the DAW library (`synth-daw` presets)? Not in this change unless the user asks.
