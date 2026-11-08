# tools/demo-songs (sidecar: full detail of the AGENTS.md row)

Pull-only (not auto-injected).

## Files
- `songs.json`: manifest `{ sources, songs }`. Sources `wavesabre` (`WS_SONG_DIR`, convert `ws`, MIT), `tunefish` (`TF_SONGS`, convert `tfm`, GPL-3.0-or-later), `enigma-studio-4` (`ES4_SONGS=<Enigma-Studio-4/demos/zoom>`, convert `tf4m`, CC0-1.0); each carries copyright, upstream repo url (pinned commit), `pipeline` and `differences` text. Seven `songs`: global-groove, elysian, engage, eladamri, no-remorse, tf4testsong, turtles (`name`, `source`, `file`, `original`, `title`, `author`, `about`). Also read by `tools/unisynth-parity/parity.ts` (`referenceSongs`).
- `build.ts`: `bun tools/demo-songs/build.ts [name ...]`. Builds `synth-convert` + `song` (release), then per song `synth-convert ws|tfm|tf4m` → `song import --meta` → `song check`, writes `project/music/<name>.song.json`. Skips songs whose source file is absent (prints `skip`), exit 1 on failure. Exports `meta(m, e)` (title, author, description, copyright, license, url, `adapted_from`), `info(name)`, `config(m, e)`, types `Source`/`Entry`/`Manifest`/`Info`.
- `build.test.ts`: manifest rules (unique names, known source, file type per converter), meta credits original + license + says adapted ("not the original recording"), url escaped, no invented authors.
- `meta/<name>.json` (optional; eladamri, elysian, engage, global-groove, no-remorse, tf4testsong): song `about` + `tracks: [{name, about}]` (blob order, no master), merged by `config()` into the `--meta` import config. Descriptions written from an instrument analysis (original device params + note statistics) and web research.
- `names.ts`: `WAVESABRE=<checkout> bun tools/demo-songs/names.ts [name ...]` refreshes WaveSabre track names in `meta/<name>.json` from `Tests/WaveSabreConvertTests/Refs/<original>.ref` `Tracks[].Name` (master dropped, repeats ` 2`, ` 3`); keeps `about` by index. Export `refNames`.

## Contracts
- `description` always says "adapted version, not the original recording" plus the pipeline and differences.
- Authors a repository does not name say so; none are guessed.

## History
- See change: add-unified-synth, add-import-patterns.
