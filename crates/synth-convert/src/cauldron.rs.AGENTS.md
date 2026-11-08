# src/cauldron.rs (sidecar: full detail of the AGENTS.md row)

Pull-only (not auto-injected). Mini-cauldron (TBL Jizz v1 / Stash v2) patch data and its mapping to unified
instruments, effect chains and phrase songs. See change: add-cauldron-xm-import.

## Data
- `load(json)` reads `patches_jizz.json` (`records` + `order`, v1) or `patches_stash.json` (`calls`, v2) → `Patches { calls, drum_lengths }`; errors start `patch data: …` and name the field.
- `Call { name (jizz|bluish|song0), version, records, order: Vec<Slot>, flag, drum_count, after_tie }`; `Slot::{Render { slot, record }, Drums}` (Stash slot 1 = `Drums`). `Patches::call(name)`.
- `Record`: a b decay damp mul gain echo, wave variants voices, decim comb doubler hold release, pitch/accent/tie ×16. `Record::decimation(version)` (v1 2; v2 1, 2, else 4; used by `effects` and `import`), `second_saw` (voices > 0, wave not spike/noise), `untied`.
- `Version::accent()` 4 (v1) / 2 (v2). `Record::retrig(variant, step)` → `Retrig { k, a, b, accent }`, `None` on a tied step; k counts the extra retrigger at each variant start; a/b compound ×mul per retrigger, rounded to f32. `Retrig::working(version)` applies the accent.

## Instrument
- Consts `R` 35441.16 Hz, `STEP` 3530, `GLIDE` 1764.5, `ATTACK` 64, `LANE_LEAD` 128, `LATCH_WINDOW` = LANE_LEAD + 128, `LP_STAGES` 2.
- `instrument(version, &Record, ratio)` → unified values: cauldron slot (wave, voices, version, clock R·ratio, filter base/amount uncompounded, decay, damping, `mul` in `FEEDBACK`, gain), `Tuning::of(..).set`, mono legato, capacity 1 (a retrigger restarts the voice), slide `GLIDE/clock`, velocity sensitivity 0, env 1 release 5/3·fade (v1 step, v2 `release`), voice master √2 (centre pan √½ per channel), no voice clip.
- `note_byte(&Retrig)` = `2k + accent` (asserts k ≤ 127); `velocity(phrase_frame)` = 1 + eighth steps, ≤ 127; `age_byte(since)` (0 = restart, else 1 + eighth steps).
- `Tuning::of(words, ratio)`: static shift (octave + `1 + ratio_fine`) centring the record's words; word 0 ignored. `note(w)` → (note, coarse, fine, word byte) for the **unsigned** word's frequency; fine byte kept ≥ 0.2 words from a rounding edge; word byte = word − rint(encoded word) + 128 (exact for every record, ratios 1 and 2). `set(values)` writes OCTAVE/RATIO/RATIO_FINE.
- `phase2_at(&Call, record, variant, step)` replays `phase2 += round(pitch·1.02)` per frame with glides (f32 = x87 24-bit); `phase_byte(word)` = 1 + round(255·w/65536) mod 255.

## Effect chain
- `effects(&Call, record, ratio, rate, tempo)` → `Chain { devices: Vec<(kind, [mode, p…])>, latency, level, restart, echo_clamped, lowpass, lp_stages }`. κ = R·ratio/rate.
- Order: shaper clipper at full scale → v2 comb (flanger comb, K = comb/κ, latency K) + clipper full scale → echo (tempo delay, fb e, wet e/(1+e), 3·3529/κ frames as eighth-units + whole ms via `delay_time`, `level` 1/(1+e)) + clipper at 1/(1+e) → v2 doubler (flanger comb sweep `SWEEP` = 65536/0xFF3B − 1, K = ⌈SWEEP·58000/κ⌉, latency K + 1, `restart` = its index) → song0 specials → band-limiting low-pass.
- Specials: distortion wrap on song0 slot 9 only (×1.25, drive ÷ level, then level 1); slots 11/15 +48 dropped (never wrap in the original, DC shift).
- Low-pass: tempo delay as a filter (no time, wet only, no feedback, SVF high cut at R·ratio/(2·decim), 20 Hz low cut) when decim > 1, hz < 20 kHz and < 0.45·rate; `LP_STAGES` stages in series, +1 latency each; `Chain::lowpass` = (first device, stored Nyquist), `lp_stages`. `lowpass_p(hz)` = tempo-delay SVF law. `slide_lowpass` adds it to a sliding track.
- `delay_time(ms, tempo)` → (coarse, fine, clamped): unit = eighth of 120/tempo s (≤ 16), rest whole ms (≤ 200); half-step-up bytes capped at 1 so every chain value stays a 0..1 parameter (`song check`); shared with `compact`. `Chain::echo_clamped` = echo ms past the delay's longest (plays at the longest, no panic). See change: add-tbl-demo-songs, compact-cauldron-xm-import, slim-cauldron-chain.

## Phrase
- `phrase_events(&Call, record, variant, ratio)` → `Vec<PhraseEv { at (phrase frames), what: PEv }>` sorted by time. `PEv::On { note, coarse, fine, word, retrig, phase, vel, age } | Legato { note, coarse, fine, word, from, vel } | Tune { coarse, fine, word } | Off | Glide { on }`. Untied steps retrigger; ties slide (Legato) or change bytes (Tune); v1 off at the start of a chain's last step, v2 at `hold`, ≥ `ATTACK` after the on; v2 tie glide window `GLIDE − ½` frames where a word > 32767.
- `note_lanes(evs at song frames, &mut values)` → (note lane, per-note byte `Auto`s on device 0), shared by `phrase_song` and `import::convert`: lanes `CLK_RETRIG`, `PM_OUT` (phase), `TRACKING` (age, only when ages differ), `FINE`, `FIXED_HZ` (word byte), `COARSE` (only when it varies), `CLK_VERSION` (128 glide / 255 v2, `LANE_LEAD/2` early); bytes step `LANE_LEAD` before the note (player applies automation per 100-frame sub-block).
- `phrase_song(&Call, record, variant, rate)` → (SongM, lead frames): instrument + chain on one track at 44.1 kHz tempo 125, doubler restart at the phrase start, track volume `1/level` (×½ for Jizz record 7). Used by `synth-convert xm-phrase`.

## Lanes
- `byte_lane(initial, changes)`: step lane, two points per change.
- `thin_lane(initial, changes, tol, breaks)`: within `tol` bytes at every frame; greedy line fit per run between hard steps (> tol or a `breaks` frame: exact zero-width step); `tol` 0 = `byte_lane`. See change: add-cauldron-lane-thinning.
- `latched_lane(initial, changes, window)`: bytes exact (within 0.45) through each note's window, free between; used for retrigger, phase, age. `fit_line`/`deltas` shared. See change: add-cauldron-latched-lanes.

## Tests
`cauldron/tests.rs` (inline `mod tests`); `pub mod import`.
