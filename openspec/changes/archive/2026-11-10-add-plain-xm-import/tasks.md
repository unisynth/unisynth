# Tasks

## 1. Guards and material

- [x] 1.1 Add a byte-identity test of the three TBL conversions: songs (release options, plus `--keep-layers
  --split-pitches --exact-chain`) and their `unisynth` renders. Record the hashes before any code moves.
- [x] 1.2 Write the synthetic FT2 fixture XMs from a test helper (no committed binaries), one per new replay feature:
  - volume column `0x83`/`0x92`
  - `R43`
  - `H` up and down
  - `P04`/`P40`
  - `X1x`/`X2x`
  - auto-vibrato: sine with sweep, square, and both ramps

  Render each once with `openmpt123`. Commit the onset frames and pitch tracks (cents per tick) as JSON.
- [x] 1.3 Pure: run its sample calculation in the local tooling (uninterrupted), then find its stored raw samples and
  its instrument blocks. Record the stored/generated list in `research/xm-softsynth-intros.md`.

## 2. Replay (TDD, `crates/synth-convert/src/xm/replay.rs` + `replay/tests.rs`)

- [x] 2.1 Tests first: the spec scenarios "Volume column fine slide", "Multi retrig", "Panning slide", "Auto-vibrato",
  "Unsupported volume column", plus the 1.2 fixtures.
- [x] 2.2 Volume column fine slide down/up; `supported` accepts `0x80–0x9F`.
- [x] 2.3 `Hxy` and `Pxy` with memories.
- [x] 2.4 `X1x`/`X2x` with separate memories.
- [x] 2.5 `Rxy` through the `E9x` trigger path, pinned to the openmpt fixture.
- [x] 2.6 Auto-vibrato; remove the error.
- [x] 2.7 The module doc matches the spec list. Run the TBL identity test (1.1).

## 3. Generator map and trigger→note path (`crates/synth-convert`)

- [x] 3.1 Make the conversion flow generic (design D2): a `Source` trait and `xm/convert.rs` holding the flow from
  `cauldron::import::convert_with`; the cauldron becomes a source. 1.1 stays green, byte for byte.
- [x] 3.2 `xmgen` module: `xm-gen/1` parse and validation (spec "Generator map", scenario "Map covers the XM"), and
  format detection next to the cauldron loader.
- [x] 3.3 `Engine` trait and registry. A `#[cfg(test)]` frame-counting engine. Tests: "Offset and loop",
  "Engine-neutral path", "Engine missing", "Stored and generated in one song".
- [x] 3.4 Generated triggers → notes: the generator-map source; start-position and backward bytes as `latched_lane`s
  (spec "Generated-note byte contract", design D3); loop re-entries; note-off at the end of a non-looped sample; the
  event callback for phrase engines.
- [x] 3.5 Track names for unnamed samples: the instrument name, then `Instrument <n>`, repeats numbered.
- [x] 3.6 `main.rs`: the map is accepted in the patch position; the usage text names both formats.
- [x] 3.7 Run `doubt-driven-review` on the `xm-gen/1` format and the byte contract before closing; both are frozen once
  songs use them.

## 5. Extraction and parity tooling

- [x] 5.1 `tools/xm-gen/`: unpack, uninterrupted emulation, generator-call hooks and the stored/generated marking.
  Writes `maps/{fudge-asc,quad-theta,quad-pure}.json` (scenario "Maps reproduce the sample list"). Add an `AGENTS.md`
  for the directory.
- [x] 5.2 `tools/unisynth-parity/plain-xm.ts` + test: the skip path; song-level comparison with the shared metrics
  against the reference XM (the map build's samples, written by `tools/xm-gen`); a missing engine is pending; the
  received XM against the reference for reading. Per-sample engine parity moves to the engine changes (no engine
  renders here).

## 6. Docs and close

- [x] 6.1 `research/xm-softsynth-intros.md`: provenance, packers and players, method (uninterrupted emulation, x87
  notes), a behavioural reading of both engines, the match results, and the credits research of D8. Own words; no
  copied code or tables.
- [x] 6.2 `docs/synth/unisynth.md`: "Generator maps (plain XMs)" next to "Mini-cauldron XMs", with the note byte
  contract. Update the AGENTS.md rows of every touched directory.
- [x] 6.3 `cargo test --workspace && bun test` green, and 1.1 green. (cargo: 65 suites incl. 1.1; bun: 322 pass, 2 skip,
  1 failure not of this change: `quirk-survey.test.ts` expects 442 V2 songs, 438 since commit 84fe5f4.)
- [x] 6.4 Write `proposal.md` stubs for `add-fudge-softsynth` and `add-poespack-softsynth` (engine oscillator types with start
  position, backward pass and seek-by-replay; engines with per-sample parity; song publication, NOTICE, listening-page group), linking this change.
- [x] 6.5 Run the `code-review` and `code-quality` gates, then commit. (CodeRabbit: 2 minor findings fixed, re-review
  0; no separate code-quality skill here: clippy clean on the touched code.)
