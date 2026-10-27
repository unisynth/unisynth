# ws-extract: WaveSabre songs out of released intro executables

Runs a Windows intro / executable-music `.exe` under Wine in a Linux container and scans the process memory for a
WaveSabre serialized song (WaveSabre `Docs/Serialized-song-format.md`: tempo, 44100, length, devices with chunk sizes,
MIDI lanes, tracks with automation). The packer (kkrunchy, squishy, Crinkler) unpacks the image at start, so the blob
is in memory even when the intro then fails on Wine's missing GPU or audio. Every candidate is validated by walking
the whole format, so a hit is a complete song.

```sh
docker build -t ws-dump tools/ws-extract
# jobs.txt: "<id>\t<path of the exe under the mounted dir's dl/>" per line; blobs land in out/<id>-<n>.bin
docker run --rm --cap-add SYS_PTRACE -v "$PWD":/w -v "$PWD/out":/out ws-dump sh /w/runall.sh   # JOBS=<file> to pick a list
synth-convert ws out/<id>-0.bin x.unified.bin && song import x.unified.bin -o x.song.json   # then render / open in the DAW
```

- 32-bit images run in a win32 prefix, PE32+ (x64 squishy) in a win64 prefix; `runall.sh` kills both wineservers between
  jobs, or a leftover process gets scanned with the next exe.
- Only the 2019+ format (WaveSabre open-sourced in Feb 2019; Trashpanda 2018 already uses it) is recognised. Older
  logicoma productions (Elysian, Engage, iota, Bare Faced Cheek …) store a private earlier format (device chunks
  without sizes) and are not found.
- Forks with extra device ids (13, 14 seen) extract but `synth-convert ws` refuses them.
- Some 32-bit squishy builds crash inside Wine before unpacking finishes; they need real Windows.

**Licence:** the extracted music belongs to its authors. Keep the blobs and renders outside the repository (results of
the 2026 survey are in `~/Downloads/64k-tools/ws-intros`, listed in `research/wavesabre-intros.md`); they are for
listening and engine parity only.
