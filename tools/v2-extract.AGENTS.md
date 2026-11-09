# tools/v2-extract (sidecar: full detail of the AGENTS.md row)

Pull-only (not auto-injected). V2 (Farbrausch) corpus, reference renders, oracles.

## Files
- `README.md`: purpose; env, usage in `research/v2-intros.md`; songs stay outside the repo, reference player v2redux (CC0) at `$V2REDUX`.
- `v2.ts gather|carve <exe> [name]|render [song…]|survey|check|oracles`. Env `V2_LAB` (default `~/Downloads/64k-tools/synth-lab`), `V2REDUX` (default `$V2_LAB/v2redux`, cmake build), `V2_SONGS` (default `$V2_LAB/v2songs`). `gather`: `acceptance/` (15 songs), `fr_public/`, `modland/` (v2m-player `.v2mz` gunzipped). `carve`: depack a demo exe + carve first song via v2redux `toolkit/era` (venv with unicorn/capstone). `render`: refs `ref/<song>.wav` via `$V2REDUX/build/v2dump`. `survey`: `survey.json` (version, features, sha256). `check`: sha256 vs `corpus/embedded/PROVENANCE.md`. `oracles`: builds `$V2_SONGS/.bin/{v2dumpjson,v2events,v2solo}`. Skips (exit 0) when v2redux absent.
- Exports: `ACCEPTANCE`, `PROFILES` (engine profile per song, empty), `acceptancePath`, `sha256`, `dumper`, `sequencer`, `dump(song, canon?)` (loader oracle JSON + canonical v6 bytes), `features`, `solo`, `reference`, `check`, `LAB`, `V2REDUX`, `SONGS`, type `Dump`.
- `dump.cpp` (`v2dumpjson`): v2redux `v2load.cpp` JSON (version, channels, globals, patches 89 v6 params + mod entries, speech size); `--canon <out>` writes canonical bytes (oracle for `synth-convert` upgrade).
- `events.cpp` (`v2events`): sequencer oracle, v2redux `v2seq.cpp` with stub synth entry points; prints `frame status data…` per MIDI message + `end <frames>`.
- `solo.cpp` (`v2solo song.v2m out.wav mask secs [kkrieger2004]`): v2redux render with only masked channels audible, optional engine profile; parity references + debugging.
- `cmp.ts <ours.wav> <ref.wav>`: quick V2 parity check with `unisynth-parity` song settings (onset-strength alignment, `BAND_FLOOR_DB`), cut to shorter render, exit 1 on FAIL.
- `make-demos.py [<v2songs dir>]`: every parsing `.v2m` in `survey.json`, once per sha256 (acceptance > fr_public > modland) → `project/music/released/v2/<slug>.song.json` (`synth-convert v2m` → `song import --meta`, `song check`). Meta from source only: `DEMOS` table (acceptance), fr_public path, modland `Author--title`; `PARITY` figures for the 18 checked songs; length cut to last note + 4 s (`TAIL_S`; held notes capped 16 s `HOLD_S` after last note-on) when V2's last tick runs past the music. Needs `target/release/{synth-convert,song}`.
- `v2.test.ts`: `features` names osc/ring/parallel filter/LFO env/speech; acceptance checksums (skipped without v2redux).

## Contracts
- Songs never committed from here except via `make-demos.py` output (published pending permission; `PERMITTED` authors (Teo; Agoston = Teo and Vincenzo) get their permission sentence instead of `LICENCE`, `WITHDRAWN` authors are left out). `composers.test.ts`: `composers.json` credits match the songs; every song states either a permission or the licence note.

## History
- See change: add-v2-import, unify-v2-algorithms.
