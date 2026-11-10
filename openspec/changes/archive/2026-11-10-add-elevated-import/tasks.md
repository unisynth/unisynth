# Tasks

## 1. Reference and reading

- [x] 1.1 Write `research/elevated-synth.md`: a behavioural reading of the engine in our own words (no copied code,
  comments or tables). It covers:
  - the machine opcodes, their byte layouts and the tree walk
  - note timing, the overwrite/cut semantics, `0x7F` and note 0
  - the pair oscillators, ops, pitch drop, envelope and noise
  - each effect's per-sample law, including the crossed delay and allpass
  - the final int16 conversion and the overflow quirk
  - provenance: Elevated and Atrium (pouet ids; Atrium is 50063), licence, and where each file came from

  Verify that every law in the three spec deltas cites a section of it.
- [x] 1.2 Write `tools/elevated-parity/Dockerfile` (the `ws-extract` base + `nasm`, `gcc-mingw-w64-i686`),
  `build-ref.sh` and the MIT harness `render-ref.c`:
  - the source location is `$ELEVATED_SRC`, default `~/Downloads/zenekk/TBC - Atrium`
  - Elevated's `music.asm` is fetched from the public source release into `$ELEVATED_REF`, default `/tmp/elevated-ref`
  - the harness supports `--solo <k>`

  Verify that Atrium renders to a WAV of `112·16·5410` frames under Wine, and that `--solo 0` gives a quieter, non-silent
  WAV. Record each acceptance song's pair detune range, phase shifts, segment lengths and any negative gain or stereo
  in the research doc; these feed D2/D7.
- [x] 1.2a `tools/elevated-parity/captures.sh`: the YouTube captures (Annikras HQ: Elevated `jB0vBmiTr6o`, Atrium
  `jlRoOqRs0bs`) go into `~/Downloads/64k-tools/elevated-captures`. Fetched 2026-11-10: elevated.wav 215.5 s (sha256
  c5d594c7…), atrium.wav 218.8 s (sha256 3e6f1323…; the song is 219.8 s).
- [x] 1.2b Capture check: compare the reference render of each song with its capture (the Tunefish 3 capture check in
  `tools/unisynth-parity`, reused). Verify that Atrium's reference passes timing, which confirms the local
  `music.asm` is the released song. Record the lag and band table in `research/elevated-synth.md`.
- [x] 1.3 `tools/elevated-parity/parity.ts` + `parity.test.ts`: the skip path (exit 0 with no sources) and the tolerance
  wiring to `tools/synth-parity`. Add the `tools/AGENTS.md` row. Verify with `bun test tools/elevated-parity`.

## 2. Unified instrument

- [x] 2.1 Pair slot type (`us-osc-pair`, module id 66), stored at oscillator-type position 5:
  - shape fields waveform/shift/detune/polarity per D2, two f64 phases (start 0, advance before the read)
  - the naive sine/square/saw read with `2·(t − round(t))`

  Verify these tests: `d = 1` saw = 2× naive saw; peaks at 435.6/444.4 Hz for `d = 0.99`; square sign at `x = 0` is −1;
  polarity subtracts (< −120 dBFS); goldens byte-identical with the feature on; a missing feature gives
  `UnsupportedModule`.
- [x] 2.2 ~~Subtract combine~~: replaced by the pair's polarity field (design D2, user decision 2026-11-10); tested in 2.1.
- [x] 2.3 Pitch drop (`us-voice-drop`): appended voice fields `drop.base`, `drop.rate` (D3), `Move::Drop` evaluated at
  control points. Verify:
  - 110 → 40 Hz: within 0.5 Hz of 110 at note-on, within 0.1 Hz of 40 after 1 s, at `k = 0.9995`
  - `k = 1` (rate 0) needs no module
  - goldens byte-identical
- [x] 2.4 Mono hard cut: verify the existing capacity 1 + steal oldest against the "Mono cut" scenario (test only).
- [x] 2.5 Linear timed envelope (`us-env-linear`): appended `linear` and `hold` per envelope (D4), times `5000·p²` ms,
  note-off ends it at once. Verify the "Linear stages" levels (within 0.02), "Note-off cuts" (silent within 32
  frames), and goldens byte-identical.
- [x] 2.6 `doubt-driven-review` on the frozen encodings (type position, appended indices, choice values) before
  section 4 writes songs. Record the outcome in `design.md`.

## 3. Unified effects

- [x] 3.1 EQ `svf-lfo` (`us-fx-eq-svf-lfo`):
  - add the one-step form to `flt::run` (flag in `c[4]`, under `FORMS`); verify that the voice-filter goldens stay
    byte-identical and that one step equals a hand-run Chamberlin step
  - the mode builds `[g, q, 1, –, 1]` itself and uses `Kind::SvfLp/Hp/Bp`, plus the dry mix
  - a closed-form coupled LFO (derive `ω`, `A` and the phase; prove it against the recurrence over 10⁷ samples, error
    < 1e-4·depth, or fall back to recurrence-from-seek)

  Verify the static low-pass response, the seek scenario (render from frame 441 000 equals the full render within 1e-4)
  and caller-block-size independence.
- [x] 3.2 Distortion `atan` (reusing the `v2::DistP` overdrive core with native atan, `gain1 = 1/a`, `gain2 = b`),
  `sine` and `knee`. Verify the transfer values (knee: 0.25 → 0.25, 0.9 → 0.6, −0.9 → −0.6;
  atan and sine at 5 points against `libm`).
- [x] 3.3 Delay `ping-pong` and `allpass`: the ring from the pool, length `round(p·65536)` ≥ 1, crossed laws. Verify the
  ping-pong impulse positions and gains (L 1 at 0, R 0.5 at 100, L 0.25 at 200), allpass power within 0.1 dB on white
  noise, and the `need` sizes.
- [x] 3.4 Append the mode ids to `fx::MODES` and add the features to `tools/check-unisynth-features.sh`. Verify that the
  feature gate passes (i686 core-only and wasm) and the existing effect goldens are byte-identical.

## 4. Converter

- [x] 4.1 `synth_convert::elevated` parser (D8), from synthetic `music.asm` text in the tests: sections, `DB`/`DD`,
  hex, negatives, comments, f32 from bytes. Verify these error cases each name their cause and write nothing:
  - a missing section
  - a truncated machine
  - opcode 9 with its offset
  - a pattern index out of range
  - a zero-length segment
  - stack underflow
  - more than one final entry
- [x] 4.2 Note rules: per-synth sequence offset, frames `(16r + s)·step`, MIDI note n, note 0 holds, `0x7F` → note-off,
  mono cut steal, and the note-off at `a + d + s`. Verify the "Long note over empty steps" and "Cut" scenarios on
  rendered output.
- [x] 4.3 Machine tree to tracks (D7) and the instrument and effect mapping. Verify:
  - the "Mixer", "Pair mapping" and "Effect chain order" scenarios
  - a warning for a negative stereo factor

  Settle the negative-gain device open question.
- [x] 4.4 `synth-convert elevated` command, usage line, and the `src/main.rs.AGENTS.md` and `RULES.AGENTS.md` entries.
  Verify that Atrium converts, `song check` passes, and `synth-convert features` lists only the modules it uses.

## 5. Parity, size, docs

- [x] 5.1 Run `parity.ts` on Atrium and Elevated, full song and per-channel `--solo`, plus the capture check of the
  converted songs. Fix the gaps with
  `systematic-debugging` until both pass the default tolerances. Record the metric table in
  `research/elevated-synth.md`.
- [x] 5.2 Size and speed: add rows to the unified size report for the new features, and for Atrium compiled as Rust
  (`synth-convert rust`) through `tools/check-song-features.sh --minimal`. Verify that each listed feature is needed.
- [x] 5.3 DAW check: open the converted Atrium in the DAW (`bun packages/daw/build.ts`, serve). The pair slot,
  linear envelope fields, drop and new effect modes must show and edit through the existing metadata. Fix the labels if they are
  missing.
- [x] 5.4 Docs:
  - new `docs/synth/elevated.md` (source format, mapping, parity)
  - `docs/synth/unisynth.md` (new modes, the provenance rows)
  - `docs/synth/README.md` pointer
  - the `crates/unisynth`, `crates/synth-convert` and `tools` `AGENTS.md` rows

  Verify `kb_search "elevated import"` finds them.
- [x] 5.5 Material check: `git ls-files` shows no Elevated sources, songs or WAVs. Then run
  `cargo test --workspace && bun test`.
