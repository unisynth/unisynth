# tools/tf3-parity (sidecar: full detail of the AGENTS.md row)

Pull-only (not auto-injected). Opt-in Tunefish 3 tools for change add-tf3-import. Material stays outside the repo:
the Enigma-Studio-3 checkout `$TF3_REF` (default `/tmp/tf3-ref`, pinned `85d65b6`; no licence stated, so never
committed) and the captures `$TF3_CAPTURES` (default `~/Downloads/64k-tools/tf3-captures`). Each tool skips (exit 0)
when its material is absent. Spec: `research/tunefish3-functional-spec.md`.

## Files
- `e3prj.ts`: `.e3prj` XML reader. `parseXml` (minimal element tree), `readProject(text, defaults)` (songs: patterns,
  tracks of `ne` events, pattern instances; instruments by parameter name, unknown names listed; errors name
  trackcount/row/root problems), `loadProject`, `readPreset` (`name;value` preset files), `playable`, `lengthRows`.
  `NAMES` (129 public names, spec §5 order), `RANGE` (stored value range per parameter), `fi` (round half to even, the
  intro's float → int), `quantise(index, p)` (the byte the intro stored, read back).
- `survey.ts [--write]`: `features(params)` tags on quantised values (oscillator points/modes/unison/sub, PAD profile,
  drones, noise, filters, envelopes, LFO shapes, matrix routes with amounts, effects; editor-only marked),
  `songStats(song)`, `survey(ref)` → markdown; `--write` replaces §9 of the spec.
- `build-ref.sh`: copies the synth sources into `$TF3_REF/build/tree/synth`, next to `shim/` so their `../system`
  includes resolve to it; builds `render-ref-player` (`-DePLAYER`, the intro) and `render-ref-editor` (`-DeEDITOR`).
  `TF3_OFFLINE=1` skips instead of cloning.
- `shim/system/system.hpp` (MIT): types, macros, `eArray` (vector), math with Windows/x87 semantics (`eFtoL` = `lrintf`,
  `eLog` = log10, f64 `ePow`/`eSin`), Park–Miller `eRandom`, `eRandomSeed` = `g_tf3Tick` (stands in for
  GetTickCount), `__m128` two-lane ops (left lane 3, right lane 2), FTZ, no-op threads/locks, stub `eDataStream`.
  `shim/math/math.hpp`: `eVector2`. `shim/eshared.hpp`: umbrella header for the player source.
- `render-ref.cpp` (MIT harness): reads the text song (`bpm`, `instrument ID v0…v128` parsed with `strtof`: presets hold subnormals such as 5e-42 that a stream refuses, `pattern`, `ev`, `instance`),
  builds `tfSong` + instruments, drives `tfPlayer::_processAudio` one 384-frame block at a time with a capturing
  sound output, advances `g_tf3Tick` with the render clock (start `TF3_TICK`, default 1000: measures the original's
  run-to-run variance), stops the song where the player's rounded row reaches the length (it would wrap half a row
  before `length·15/bpm`) and renders the tail; writes 16-bit stereo WAV. Song and player are never destroyed (the source's explicit destructor calls assume
  its own array type).
- `cycle-ref.cpp` / `pad-ref.cpp` (MIT harnesses, built by `build-ref.sh` into `$TF3_REF/build`): print one point
  cycle (14 point values) / one PAD table (5 additive values) of the original, for `crates/unisynth`'s opt-in
  `pts::parity` and `pad::parity`.
- `render-ref.ts <in.e3prj> <out.wav> [--editor] [--song id] [--tail s]`: `harnessText` (player mode quantises),
  `pickSong`, `renderRef` (false when the renderer is absent), `renderer(editor)`, `refDir`.
- `captures.sh`: pinned YouTube ids (transplant `31cDnVWhS7w`, pandora `RlyIJD9AEWM`, train-control `Gm0NWMdcbmU`) →
  44.1 kHz stereo WAV via `uvx yt-dlp -x`; prints SHA-256 prefix and music start (−60 dBFS); skips without uvx/ffmpeg.
- `capture.ts <ours.wav> <capture.wav|slug>`: `compareCapture(ours, cap, windowS = 30)` → `CaptureReport` (`offset`,
  `gainDb`, per-window `lag`/`corr`, octave `bands` ≤ 8 kHz, `onsets` = windows whose timing agrees, `pass`),
  `firstSound`, `formatCapture`, `CAPTURE_TOL` (4 dB, 768 frames, 90 % windows, corr ≥ 0.5, ±0.5 s). Onset curves
  (`novelty(onsetFlux)`) are computed once per whole signal; slicing per window adds a false onset at each cut.
- Tests: `e3prj.test.ts` (names/ranges, `fi`, quantise, project + errors, preset, features, survey skip),
  `render-ref.test.ts` (`oneNoteProject` helper; skip paths; one-note render non-silent when the renderer exists),
  `capture.test.ts` (first sound, delayed quieter copy passes, other material fails timing, skip paths).

## Results (2026-11-09)
- Transplant reference render vs its capture: player mode ≤ 0.20 dB per octave band, constant lag (offset 1.403 s),
  window correlation 0.71–0.89, PASS; editor mode ≤ 1.30 dB. The released intro is the player build.
- Survey: Transplant plays all 11 instruments (2096 rows at 140 bpm, 2348 notes); editor-only features in played
  instruments: an inert LFO → BP route (instrument 9) and an EQ (instrument 10).
- See change: add-tf3-import.
