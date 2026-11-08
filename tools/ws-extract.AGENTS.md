# tools/ws-extract (sidecar: full detail of the AGENTS.md row)

Pull-only (not auto-injected). WaveSabre song blobs out of released intro executables.

## Files
- `Dockerfile`: debian bookworm-slim + wine32/64 + xvfb + python3; win32 prefix `/root/.wine32`.
- `dump.py <exe> <name> [secs]`: runs the exe under Wine (xvfb), scans `/proc/<pid>/mem` in 64 MB chunks (4 MB overlap) for the 44100 Hz header, validates by walking the whole serialized format, writes distinct blobs to `/out/<name>-<n>.bin` (largest first). PE32+ uses a win64 prefix (`/root/.wine64`). Stops ~6 s after the first hit.
- `runall.sh`: `JOBS` list (`jobs.txt`, lines `<id>\t<path under dl/>`); skips ids with output, kills both wineservers between jobs.
- `make-demos.py [<ws-intros dir>]` (default `~/Downloads/64k-tools/ws-intros`; needs `out/<id>-<n>.bin`, `catalog.json`, `details.json`, `synth-lab/ws/gm.dls` for wave names, `target/release/{synth-convert,song}`): writes the 25 parity-checked songs (`SONGS`, incl. Robo Outer Space Epos via fork kind 13 Screwdriver) to `project/music/released/<slug>.song.json` (committed, published pending permission) with `--meta` (title, music credit, description: party placing, tempo/length/tracks/notes, WaveSabre instruments and effects, GM.DLS note, adapted/parity text, no licence) and derived track names `<Role> (<Instrument>|GM <wave>)` with role Drum/Bass/Lead/Chords/High (from notes and GM wave name), `Bus` without an instrument.
- `README.md`: usage; limits (2019+ format only, fork device ids 13/14 seen — README says `synth-convert ws` refuses them, but 13 (Screwdriver) converts now, see `make-demos.py`; some 32-bit squishy builds crash in Wine); music belongs to its authors.
