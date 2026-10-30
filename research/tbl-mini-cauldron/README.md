# TBL mini-cauldron (Jizz / Stash 1997): reverse engineering

The Black Lotus' 64k intros Jizz and Stash play ~1 MB FastTracker 2 XMs.
Almost all of the sample data is synthesized at startup by **mini-cauldron**,
Probe's ~1.4 KB x87 "sequencer-phrase" synth, into an XM template that
ships in the executable. This folder holds the full algorithm spec, decoded
patch settings, a bit-exact Python reference and the toolchain that proves it.

- Spec: [`mini-cauldron-spec.md`](mini-cauldron-spec.md) (start here)
- Reference: `ref/mcauldron1.py` (Jizz, v1), `ref/mcauldron2.py` (Stash, v2), `ref/x87pc24.py`
- Data: `ref/patches_jizz.json`, `ref/patches_stash.json`, `ref/golden_sha256.json`
- Tools: `tools/` (unpack, emulate, trace, disassemble, verify)

## Result

| Intro | Generated samples | Reference vs original code |
|---|---|---|
| Jizz | 32 × 28 000 B from 12 patches | 32/32 byte-identical |
| Stash | 53 renders (14 000/29 000/59 000 B) from 26 patches + 13 stored drum copies | 66/66 byte-identical |

The emulated Jizz XM is byte-identical to the official rip on scene.org
(`demodulate/jizz.zip`), so the emulation oracle is trustworthy.

## Reproduce

```bash
research/tbl-mini-cauldron/tools/run_all.sh     # ~2 min; needs python >= 3.13, curl
```

It downloads `jizz.ixa` / `stash.ixa` from the official iXalance archive
(libsdl.org) into `out/` (git-ignored), unpacks and relocates the DOS32 images,
runs each intro under unicorn until the music starts, extracts the patch
tables, renders with the references and compares byte for byte. Finally it
rebuilds the XMs from the reference renders and, if `openmpt123`
(`brew install libopenmpt`) and ffmpeg exist, renders
`out/listen/*.mp3`. Open `out/listen/index.html` to listen.

## Notes

- Everything copyrighted stays local: the intro images, XMs and rendered audio
  go to `out/`. The repo holds the spec, decoded parameter tables, hashes and our
  own code. The user confirmed this kind of analysis is legal for us.
- All addresses in the docs assume the image loaded at 0x400000 (`out/<intro>/reloc.bin`).
