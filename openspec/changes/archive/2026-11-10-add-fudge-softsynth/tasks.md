# Tasks

## 1. Guards and review

- [x] 1.1 Baseline: `cargo test -p synth-convert --test tbl_identity` and the unisynth goldens are green. Record the
  `bun test` baseline (one known failure: `quirk-survey.test.ts`, 442 vs 438).
- [x] 1.2 `doubt-driven-review` on design D1–D3 (positions, the clock from pitch, the laws) before any song uses them.
  Fresh-context review (@review), 2026-11-10. It found:
  - the slope cannot round-trip exactly;
  - drift from inverting the drum recurrence;
  - the v3 hook placed before `levels()`;
  - the integer-flag decode;
  - the `FIXED_HZ` branch;
  - the range numbers.

  All are folded into D2–D5. Positions, the carried value at frame 0 and the unused fourth argument were confirmed.

## 2. Engine frame functions (TDD, `crates/unisynth/src/osc/engine.rs`)

- [x] 2.1 Tests first:
  - the shaper thresholds and the fold;
  - frame 0 resets the carried value;
  - the three envelope curves and a rising envelope;
  - the law round-trips (slope within 2⁻²⁸; integer flags read as integers);
  - seek ≡ replay, and backward ≡ reversed forward (within 1 LSB), on synthetic parameters.
- [x] 2.2 `ToneT`/`DrumT` from the shape values (D3), the f32 store truncating toward zero, the frame functions, and
  the fixed-size `EngineSlot` state (index, direction, carried value, drum `freq`/`sweep`).
- [x] 2.3 Tone: closed-form `v_raw(i)`, scan-back for the carried value. Drum: replay seek and the 64-frame backward
  block (exact forward replay).
- [x] 2.4 Frame-level parity (D6.1) against `$ZENEKK_DIR/xm-gen/fudge-asc.reference.xm`. Skip without it. 22/22
  within target, including a backward pass per ping-pong loop and the dropped fourth argument shown to change
  nothing. On a gap, use `systematic-debugging`.

## 3. Engine types in the slot (`crates/unisynth`)

- [x] 3.1 Tests first:
  - `kinds_follow_the_type_parameter_and_the_build` extended to 7 and 9;
  - the missing-module load;
  - speed follows pitch;
  - voice level (D6.2): clock = rate → frame function delayed by 2 frames;
  - the start byte, the backward byte and legato;
  - a position byte ≥ 128 does not switch the slot to a fixed frequency.
- [x] 3.2 `params.rs`: `osc_type::SHAPER_TONE` = 7 and `SWEEP_DRUM` = 9, `is_gap`, `shape::TONE_*`/`DRUM_*`. Also:
  `module.rs` (75, 76), `needs.rs`, `Cargo.toml` (features enable `us-osc-clocked`; added to `us-osc`),
  `tools/check-unisynth-features.sh`.
- [x] 3.3 `osc.rs`: `Kind::ShaperTone`/`SweepDrum`, the arms in `sample` through the clocked resampler (shared helper),
  and send 0. `voice.rs`: clock `256·hz/rate`, a non-legato note-on reads the contract bytes and seeks.
- [x] 3.4 `crates/song/src/tables.rs`: labels "Shaper tone" and "Sweep drum" at 7 and 9, and the shape names.
- [x] 3.5 Goldens byte-identical: `cargo test -p unisynth`.
- [x] 3.6 Note-on budget (`performance-optimization`): a release-build measurement over the song's 1869 offsets and
  every ping-pong entry. Record the numbers in `design.md` D4. Add the last-seek cache only if the 50 µs budget fails.

## 4. Converter (`crates/synth-convert`)

- [x] 4.1 Tests first:
  - argument decode, range and exactness errors (scenario "Out-of-range argument");
  - the clock → octave/coarse/fine split;
  - engine lanes at v3 indices (scenario "Engine lanes");
  - the converted song loads (scenario "Converted song", with `$ZENEKK_DIR`).
- [x] 4.2 `Source::v3_type` (default `None`). In `xm/convert.rs`: run `to_v3` after assembly when any track has a type,
  then set `o::TYPE` of slot 1 on those devices (D5). TBL identity stays green.
- [x] 4.3 `xmgen/fudge.rs`: `ShaperTone`, `SweepDrum`; register them in `engines()`.
- [x] 4.4 `plain-xm.ts`: the Fudge row is judged (D6.3); update the report and its test.

## 5. DAW (`packages/daw`)

- [x] 5.1 Tests first (`params.test.ts`, device tests):
  - `oscType` 16/17;
  - `fmtVal` units;
  - the common fields per type;
  - defaults on a type change in one undo step;
  - other types unchanged;
  - the drawing against engine fixtures.
- [x] 5.2 `model/params.ts`: kinds 16/17, `fmtVal` for the engine fields, `engineFrames` (a TS mirror of the frame
  functions for the drawing), `typeDefaults(kind)`.
- [x] 5.3 `device/Device.tsx`: `OSC_SHAPES` 16/17 and `OSC_COMMON_BY_TYPE`. `app/device.ts`: `setParam` writes the type
  defaults with the type change. `app/screens.ts`: the engine drawing.
- [x] 5.4 `bun packages/daw/build.ts`, then open the converted song in the DAW and switch a slot between Phase mod,
  Sweep drum and Pair (scenarios "Switch to a drum", "Back to another type", "Converted Fudge").

## 6. Publication

- [x] 6.1 `project/music/released/fudge-alien-sex-clone.song.json` with the meta (Simon / Simn, Demozoo 21494 and
  202313, pouet 444), and a `NOTICE.md` entry.
- [x] 6.2 `tools/song-page/build.ts`: the group "GUS 64K intros, 1998 (softsynth XMs)", plus its test.
- [x] 6.3 `scripts/build-daw.sh && scripts/build-songs.sh` (with `GM_DLS`), then listen in the DAW and on the songs
  page.

## 7. Docs and close

- [x] 7.1 `docs/synth/unisynth.md`: "Engine oscillator types" (positions, clock, laws, seek). Update the AGENTS.md rows
  and sidecars of every touched file.
- [x] 7.2 `cargo test --workspace && bun test` green (apart from the known baseline failure), and the TBL identity
  green.
- [x] 7.3 `code-review` gate, then commit. (CodeRabbit, three rounds: 5 minor findings fixed — engine-type defaults reset the
  hidden start/direction bytes, NaN period draws silence, `make-demos.py` fails on a failed check or missing log line,
  stale proposal sentence; 1 false positive: the songs page already versions the wasm by content. Archive
  `add-plain-xm-import` first: this change's MODIFIED requirement extends its spec.)
