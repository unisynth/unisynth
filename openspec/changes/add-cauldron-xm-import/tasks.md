# Tasks

## 1. Reference fixtures and parity harness (first, so every later group can measure)

- [x] 1.1 `research/tbl-mini-cauldron/tools/rebuild_xm.py`: also write per-phrase references resampled to 44.1 kHz
      (`out/listen/ref44/<name>.wav`) and a `phrases.json`. The JSON gives each phrase's (song, record, variant,
      decim, native ratio, stored Nyquist). Verify: `run_all.sh` still prints `ok 32 bad 0` / `ok 66 bad 0` and writes
      85 reference WAVs.
- [x] 1.2 Render the three song references with `openmpt123` at 44.1 kHz into `out/listen/ref44/`. Verify: durations
      166.3 s / 203.5 s / 84.3 s ± 0.1 s.
- [x] 1.3 `tools/unisynth-parity`: add a `cauldron` suite.
      - Song mode: three songs, the shared metrics.
      - Phrase mode: each phrase, with bands above the stored Nyquist excluded.
      - Ping-pong seconds reported per song.
      - Skips with a message when `research/tbl-mini-cauldron/out` is absent.

      Verify: `bun test tools/unisynth-parity` passes the skip test and a self-comparison (reference vs itself: all
      metrics 0).

## 2. Cauldron oscillator (`unisynth`)

- [x] 2.1 Doubt-driven review before coding: the stored position (`osc_type` id 6, odd position 3 of the expanded
      choice), the shape-field layout (wave, voices, version), and the module id appended in `module.rs`. Record the
      outcome in design.md D5.
- [x] 2.2 Tests first in `osc.rs`:
      - saw vs the original 16-bit saw, sample for sample, at pitch 799 / R = 35 441.2 Hz
      - v1 spike and v2 spike2 vs `ref/mcauldron{1,2}.py` waveform functions on dumped vectors (store a few hundred
        values as a test table)
      - square, pulse, fold, the voices pair, note-on phase −32766, legato keeps the phase
      - missing-feature load error

      They must fail before 2.3.
- [x] 2.3 Implement `us-osc-cauldron`. Add it to the `us-osc` and `us-all` groups, `needs.rs`, `module.rs`, and the
      host parameter table. Verify: the 2.2 tests pass; `cargo test -p unisynth` passes; the existing goldens are
      byte-identical.
- [x] 2.4 Docs: `docs/synth/unisynth.md` gets a "Cauldron (`us-osc-cauldron`)" oscillator section and a parameter-table
      row; `crates/unisynth/AGENTS.md` rows (`osc.rs`, `module.rs`, `Cargo.toml`). Verify: `kb_search "cauldron
      oscillator"` finds them.

## 3. Comb and wrap effect modes (`unisynth`)

- [x] 3.1 Tests first:
      - comb nulls a sine at `rate/K`
      - sweep restart resets the read offset
      - doubler equivalence: on a reference phrase, the realtime sweep matches the in-place doubler of
        `mcauldron2.py` (ported into the test) within 1 dB per octave band (checked sample-wise: error ≥ 20 dB below the signal, which bounds every band)
      - wrap example 0.9·1.25 → −0.875
      - old mode values decode unchanged: shaper 0, curve 0.5, power 1.0, phaser 0, ring 1.0

      They must fail before 3.2.
- [x] 3.2 Implement `fx/comb.rs` (flanger mode 2) and `fx/wrap.rs` (distortion mode 3), the `modes!` entries and
      features. Extend `mode_of_value`'s trusted-build shortcut with the new features. Verify: 3.1 passes;
      `cargo test -p unisynth`; the effect goldens are byte-identical.
- [x] 3.3 Docs: the effect table in `docs/synth/unisynth.md` and the `crates/unisynth/AGENTS.md` rows. Verify: both
      modes are listed with their parameters.

## 4. Mini-cauldron instrument mapping (`synth-convert`)

- [x] 4.1 Read `patches_*.json` (move `serde_json` to `[dependencies]`) into typed records. Verify: a unit test loads
      both files and checks the record counts 12 / 10 + 16 and Jizz record 0's values against the spec table.
- [x] 4.2 Retrigger counter and cutoff ramp: given (record, variant, step), return k and (A_k, B_k), with the
      double retrigger at each variant start. Verify: a test against a trace of the reference renderer's
      retrigger calls (`mcauldron1.py` instrumented in a test helper) for every Jizz record.
- [x] 4.3 Filter (D4, revised): the fit measured 5 records at 2.7–4 % and 7 with a rising cutoff that no envelope
      makes, so the original filter became part of the cauldron slot (shape 5–8). Verify: reference filter vectors
      (`tools/filter_vectors.py`) within 1e-4 of peak, κ = ½ step response within 1 %, filtered golden added and
      existing goldens unchanged.
- [x] 4.4 Instrument builder (`cauldron::instrument`): cauldron slot with its filter (uncompounded base and amount,
      `mul` in `FEEDBACK`; per note a retrigger byte `2k + accent` and the phrase position as the velocity, D3
      revised: automation is one byte per point), voice clip at full scale, mono legato with slide = 1764.5 frames at
      the ratio, note-end release (energy-matched). The Nyquist low-pass moved to the effect chain (4.5). Verify:
      every record's retriggers give the reference filter values (1e-5), voice settings decode at ratios 1 and 2, all
      76 instruments load and play within full scale.
- [x] 4.5 Effects per track (`cauldron::effects` → `Chain { devices, latency, level, restart }`): comb, delay (tempo
      mode, 3 steps), sweep with restart lane, wrap for the Stash specials, then the Nyquist low-pass for decim > 1
      (last, D6), in the D7 order. Verify: tests check chain order, comb/echo/doubler/low-pass times at ratios 1 and
      2, the echo level folded into the wrap drive or reported, and every record's instrument + chain loads and plays.
- [x] 4.6 Per-phrase converter entry (`synth-convert xm-phrase <patches.json> <call> <record> <variant> <out.song>`, call = jizz | bluish | song0, also used
      by the parity suite): one trigger at native speed. Verify: the parity phrase suite passes for all 32 Jizz
      phrases. Iterate on 4.4 here; record any deviation that cannot pass, with its cause, in design.md.
      Done: 32 of 32 Jizz phrases pass. Fixes found on the way (design D3–D7, D11): the slot runs at its phrase
      clock and resamples; the clip is the first effect, after the release; one voice per instrument; instant
      attack, frequency-linear slide and a per-note second-saw phase byte for cauldron voices; the bandwidth
      low-pass is the tempo delay as a filter; the harness judges our mid channel against the mono references.
- [x] 4.7 Same for the 53 Stash phrases. Verify: the parity phrase suite passes, or each failure is listed with its
      cause in the report and design.md before moving on.
      Done: 48 of 53 (Jizz 32 of 32; 80 of 85 overall), the five below documented as known deviations. Fixed: 8-bit clamps after the comb and the echo;
      decimated phrases judged against the alias-free reference; unsigned pitch words (per-record shift + coarse
      lane); the glide's signed register (`CD_VERSION` ½); the word byte (`o::FIXED_HZ`); bands judged only wholly
      below the stored Nyquist. Known deviations, with causes (design D5, D6):
      - song0 r3 (word 51199 held 16 steps, decim 4): the word byte (`o::FIXED_HZ`) made the voice exact (comb stage
        within 1.6 dB in every band); the miss enters at the echo (250 Hz −3.7 dB): one steady tone through a 0.5
        feedback echo interferes with itself, and our echo is 0.25 ms late (whole-ms delay law; ≈1 dB), its cut
        filters shift phase per pass, and the circular wrap of steps 13–15 into 0–2 is missing (design D5).
      - song0 r4: rising cutoff passes c ≈ 1 at step 12, where the recurrence turns unstable; the original's integer
        `prev` (fistp, 32-bit) bounds it differently from our float (steps 0–11 match in level and bands).
      - song0 r10 (+48): the voice matches (corr 0.99, bands 0.0); the excess was a wrap that never fires in the
        original (DC shift only), now dropped; left: +37 dB at 8 kHz, above its 4.4 kHz stored Nyquist.
      - bluish r7 v3, r8 v2: onset correlation 0.48/0.50; loudness and bands within tolerance.

## 5. XM reader and control replay (`synth-convert`)

- [x] 5.1 XM reader: header, orders, patterns, instruments, sample headers, stored PCM. Reject non-linear frequency
      tables and unknown header sizes. Verify: tests on a synthetic XM writer and every truncation of it.
- [x] 5.2 Tick replay with FT2 semantics for the effect set in the spec; unsupported effects are an error naming
      pattern/row/channel. Verify: unit tests per effect on synthetic patterns (note delay, cut, retrigger, offset,
      portamento, envelopes with sustain/loop, fadeout, pattern break/jump); an onset-time comparison on the three
      songs against the reference renders (onsets within 100 frames) using silent placeholder instruments plus click
      markers.
- [x] 5.3 Phrase unrolling: phrase time integrated in f64 from `r(t)`; notes per step; ties as legato; `9xx`/retrigger/
      loop jumps resume at the step under the position; ping-pong backward passes in reversed step order with one
      warning per sample. Verify: the spec scenarios (tied slide, octave-down, later variant brighter, tone
      portamento, pad past its end) as converter tests.
- [x] 5.4 Track grouping (record, channel, ratio class) with the merge rule; gain/pan/bend automation (D10). Verify:
      a test with the same record on two channels at two ratios gives two tracks with the right echo times; the track
      count per song is printed.
- [x] 5.5 Stored samples as GSM sampler instruments with their loops (D8). Verify: the "drum kit" scenario test;
      sampler loop settings equal the XM's.

## 6. Songs and parity

- [x] 6.1 CLI `synth-convert xm <in.xm> <patches.json> <out.song>` with errors writing nothing. Verify: CLI tests for
      success, the unsupported-effect error, and no output on error.
- [x] 6.2 Calibration constant (D10) from the phrase suite, then the song suite on Jizz. Verify: Jizz passes all three
      metrics; the report is committed (`tools/unisynth-parity/cauldron-report.md`, metrics only).
- [x] 6.3 Stash song 1 and BLUISH. Verify: both pass, or each failure is listed with its cause in the report; the
      ping-pong seconds are reported.
- [x] 6.4 `research/tbl-mini-cauldron/tools/run_all.sh`: when `synth-convert` is built, convert the three songs into
      `out/songs/` and render them with the unified player for listening (mp3 next to the references). Verify:
      `run_all.sh` ends with the three converted mp3s.
- [ ] 6.5 A/B listen by the user (references vs converted), with sign-off recorded like the earlier suites
      (`cauldron-signoff.json`). Verify: the sign-off file exists with the user's verdict.

## 7. Docs and closeout

- [ ] 7.1 `docs/synth/unisynth.md`: a converter section "Mini-cauldron XMs" covering the mapping table, D2 track
      rule, the ping-pong approximation, and the walkthrough commands. `research/tbl-mini-cauldron/mini-cauldron-spec.md`
      §9 points to it. Verify: the walkthrough commands run as written.
- [ ] 7.2 `crates/synth-convert/AGENTS.md` rows for the new modules and CLI; `research/tbl-mini-cauldron/AGENTS.md`
      updated. Verify: `kb_search --doc-type agents "xm import"` finds the rows.
- [ ] 7.3 Speed and size: per-feature size cost of `us-osc-cauldron`, `us-fx-flanger-comb`, `us-fx-distortion-wrap`
      in the size report, and the render speed of the three songs. Verify: `tools/unisynth-parity/size.md`
      lists them.

## Workflow follow-up

- Run `code-review` and `code-quality` gates before commit.
- Archive with `openspec archive add-cauldron-xm-import` after the sign-off.
