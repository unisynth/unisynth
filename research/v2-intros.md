# V2 acceptance songs (change add-v2-import)

The 15 songs that `tools/unisynth-parity` checks. The 13 demo songs were carved by v2redux from the release
executables at their own format version (`corpus/embedded/PROVENANCE.md`). pzero and v2_zeitmaschine are the two
version 6 files in fr_public `v2/v2m/`. The songs are © their authors (Farbrausch / .theprodukkt). The `.v2m`
files and their renders stay in `$V2_SONGS` (default `~/Downloads/64k-tools/synth-lab/v2songs`). Their conversions are
committed as DAW songs in `project/music/released/v2/` (`tools/v2-extract/make-demos.py`) while the authors are asked
for permission.

Reproduce:

```bash
bun tools/v2-extract/v2.ts gather   # acceptance/, fr_public/, modland/ (360 files; .v2mz unzipped)
bun tools/v2-extract/v2.ts check    # acceptance sha256 vs PROVENANCE.md
bun tools/v2-extract/v2.ts render   # ref/<song>.wav via v2redux v2dump
bun tools/v2-extract/v2.ts survey   # survey.json: version + features per song
bun tools/v2-extract/v2.ts carve <demo.exe> [name]   # carved/<name>.v2m (venv with unicorn/capstone made on first use)
```

The carving path is verified. fr-08 `fr08v101.exe` (scene.org `parties/2000/theparty00/in64/fr08_final.zip`)
carves to `36e0546d709cca86…`, the PROVENANCE checksum.

| Song | Demo | ver | Length s (ref render) | sha256 (prefix) | Features (survey) |
|---|---|---|---|---|---|
| fr08 | fr-08 .the .product | 0 | 660.9 | `36e0546d709cca86` | 19: saw/tri pulse sin noise, LP/BP/HP/notch/all-pass, OD/clip, S+H, chorus, channel dist |
| flybye | fr-013 flybye | 1 | 144.3 | `1ebaf51d6e71ad44` | 18: + boost, sum compressor |
| fr014 | fr-014 mark & sweep | 3 | 136.3 | `d505ce35fd29528f` | 20: + channel compressor, crush |
| farbomat | fr-minus-03 farbomat | 3 | 111.5 | `87ac3a7cbb56e01d` | 18: + ring, parallel filters, pulse LFO |
| fr022_party | fr-022 ein.schlag (party) | 3 | 360.8 | `a7b279bb95d92334` | 24: + channel all-pass |
| fr019 | fr-019 poem to a horse | 4 | 1200.0 (cap: song loops, render stops at 1200 s) | `9c7e7e07bead2067` | 27: + FM, decimator |
| brullwurfel | fr-028 brüllwürfel | 5 | 216.0 | `75a6192810da581e` | 9: the simplest song |
| candytron | fr-030 candytron | 5 | 197.6 | `5bf17fb8d58d608c` | 30: + speech, Moog-free, NoF |
| fr022_final | fr-022 ein.schlag (final) | 5 | 150.4 | `54c5b6982be94402` | 13: early-v5 build (v2redux: era-inaccurate) |
| fr024 | fr-024 | 5 | 210.2 | `0d2ae1f73a1f6841` | 19 |
| fr027 | fr-027 out of the blue | 5 | 361.4 | `3aa68bf67800b9a4` | 22: early-v5 build (v2redux: era-inaccurate) |
| fr029 | fr-029 | 5 | 182.8 | `eb33d567ab798534` | 23 |
| kkrieger | .kkrieger (beta) | 5 | 200.2 | `a497ccb3c4626d5b` | 28: + speech (v2redux `eras::kkrieger2004` straddles builds) |
| pzero_new | fr_public `v2/v2m` | 6 | 235.4 | `c39e692de35b4ab6` | 28: + speech |
| v2_zeitmaschine_new | fr_public `v2/v2m` | 6 | 185.3 | `a718360b4e938211` | 21: + speech |

The full per-song feature lists are in `$V2_SONGS/survey.json`. The whole local corpus has 360 files, 359 of which
parse: versions 0 ×4, 1 ×8, 3 ×8, 4 ×3, 5 ×296, 6 ×40.
