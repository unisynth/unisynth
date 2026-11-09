# tf-extract: Tunefish 4 songs out of Brain Control's intros

Runs an intro `.exe` under Wine in the `ws-dump` container (`tools/ws-extract/Dockerfile`) and scans the process memory
for a Tunefish 4 `.tfm` song (`docs/synth/tunefish4.md`: `u16` instrument count ≤ 32, `u16` tempo, event counts,
`INST`, 112 bytes per instrument, `SONG`, events, `ENDS`). kkrunchy unpacks the image at start, so the song is in
memory even when the intro then fails on Wine's missing GPU or audio. A hit is accepted only when the counts land
exactly on `SONG` and `ENDS`.

```sh
# dl/ holds the unpacked intros (braincontrol.org zips wrap an encrypted.zip, password "braincontrol")
# run in <dir> (default ~/Downloads/64k-tools/bc-intros); T = this directory
docker run --rm -v "$PWD":/w -v "$PWD/out":/out -v "$T":/t ws-dump sh -c \
  "cp '/w/dl/<intro>.exe' /tmp/a.exe && timeout 120 python3 /t/dump.py /tmp/a.exe <slug> 90"   # → out/<slug>-0.tfm
python3 tools/tf-extract/make-demos.py [<dir>]    # → project/music/released/<slug>.song.json
```

`RAW=1` also writes the writable memory regions to `out/<slug>.raw` (blocks of `u64 address, u64 length, bytes`) for
formats the scanner does not know.

Brain Control's 64K intros, by engine:
- Tunefish 4 (open-source version): Aten Hotep (Revision 2019), Naumachia Insianis (Revision 2024). Extracted,
  parity-checked against the upstream player (`tools/tf-parity`), imported.
- Tunefish 4 of 2013 (Enigma Studio 4): Turtles all the way down. Its project is public under CC0
  (`project/music/turtles.song.json`).
- Enigma Studio 3 / Tunefish 3: Pandora (2011), Transplant (2011), Train Control (2012). Another engine (source in
  `enigmastudio/Enigma-Studio-3`, no licence stated; spec `research/tunefish3-functional-spec.md`). Transplant's
  project ships in that repository and is read by `tools/tf3-parity` (change add-tf3-import). This scanner does not
  read Tunefish 3 songs (no magic bytes); `tools/tf3-extract` does, without Wine (Transplant, Train Control; Pandora's
  older layout is a follow-up). YouTube captures of all three intros serve as the acceptance reference
  (`tools/tf3-parity/captures.sh`).
- 2003-2009 intros: older engines, not surveyed. The games Lick Attack and Space Driver hold no `.tfm`.

**Licence:** the extracted music belongs to its authors. Keep `.tfm` files and renders outside the repository; the
imported songs are published only pending Brain Control's permission (`project/music/NOTICE.md`).
