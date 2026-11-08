# research/tbl-mini-cauldron/tools

Toolchain to unpack, emulate, trace and verify TBL iXalance intros. Python; unicorn + capstone in `out/venv` (created by `run_all.sh`). Addresses assume BASE 0x400000.

| File | Purpose |
|---|---|
| `disbb.py` | Disassemble executed blocks from `bb.json` in [lo, hi) (capstone); gaps = data/unexecuted. |
| `disx.py` | Linear disassembly of `reloc.bin` at an address; beware inline constants. |
| `extract_patches.py` | Reads `out/{jizz,stash}/reloc.bin` → `../ref/patches_{jizz,stash}.json` + `out/stash/drums.json`. Hard-coded table addresses (v1 0x41175A/0x41172A, v2 0x4037ED/0x40317D, drums 0x403BF7/0x408C7C). |
| `filter_vectors.py` | Filter vectors from `ref/mcauldron{1,2}.py`: the reference `frame()` at constant pitch, gain 1, one retrigger (prev 0, new v1/v2, y runs on), filter output per frame → `crates/unisynth/src/osc/cauldron_filter_vectors.rs`. See change: add-cauldron-xm-import. |
| `fullrate_phrases.py` | Alias-free phrase references: hooks `post` of `mcauldron1`/`mcauldron2` to take each phrase at the full rate before the decimation (after Jizz record-7 halving / Stash specials), FFT brick wall at the stored Nyquist + resample to 44.1 kHz → `out/listen/ref44bw/<phrase>.wav` (mono 16-bit). `capture_v1`, `capture_v2`, `band_limit`. Run by `run_all.sh` (~1 min, numpy). See change: add-cauldron-xm-import. |
| `ixa_host.py` | `Host`: unicorn x86-32 flat host of the iXalance ABI (far-call entry stack, farmalloc/fardoint/farshowp trampolines, GDT flat selectors 0x08/0x10). Stops at first `'TBL1'`, saves every XM in memory as `xm_<addr>.xm`. TBL3 fast-forwards music position. |
| `make_demos.py` | The three soundtracks as DAW demo songs in `project/music/released/` (committed pending the authors' permission): `tbl-jizz-smokeout`, `tbl-stash-over2`, `tbl-stash-bluish`; `synth-convert xm … --names` → `song import --meta` (title, author Probe (TBL), adapted-version description, © 1997 TBL, pouet URL, Demozoo `adapted_from`, track names). Needs `out/listen/*.xm` and release `synth-convert`, `song`. Converts with `--thin 2` (`THIN`). See change: add-cauldron-latched-lanes. |
| `osc_vectors.py` | Oscillator-only vectors from `ref/mcauldron{1,2}.py` (constant pitch, retrigger phase, t0) → `crates/unisynth/src/osc/cauldron_vectors.rs`. See change: add-cauldron-xm-import. |
| `phase2_trace.py` | The second saw's phase at every retrigger of full reference renders (hooks `retrigger`/`segments`, records by raw bytes) → `crates/synth-convert/tests/data/cauldron_phase2.json` (~1 min). See change: add-cauldron-xm-import. |
| `probe_state.py` | Print RNG state / FPU CW / dwords at first execution of an address. |
| `rebuild_xm.py` | XM templates + reference renders → `out/listen/{jizz,stash_song1,stash_bluish}.xm` (byte-identical to emulator XMs), `phrases/*.wav` at native rate R/decim, `phrases.json` (song, record, variant, decim, rate, Nyquist per phrase), `ref44/<phrase>.wav` (44.1 kHz parity references, ffmpeg), `index.html`. `write_index` adds a unisynth player next to each reference song when `<song>.unisynth.mp3` exists (A/B). See change: add-cauldron-xm-import. |
| `retrigger_trace.py` | Retrigger calls of the references (no audio): per rendered record, (variant, step, A, B, v1, v2) → `crates/synth-convert/tests/data/cauldron_retriggers.json`. See change: add-cauldron-xm-import. |
| `run_all.sh` | End-to-end: fetch `.ixa` (libsdl.org), unpack, emulate, cross-check jizz.xm vs scene.org rip, extract, render, verify, rebuild XMs; mp3 via `openmpt123` + ffmpeg if present. Writes `out/listen/ref44bw/` via `fullrate_phrases.py` (venv gets numpy). Last step: with `target/{fast,release}/synth-convert` and `examples/render` built, converts the three XMs to `out/songs/<song>.song` (+ `.log`), renders, trims `lead-frames`, writes `out/listen/<song>.unisynth.mp3`, refreshes `index.html`. See change: add-cauldron-xm-import. |
| `trace_bb.py` | Basic-block coverage until TBL1 → `bb.json` {addr: [count, size, first order]}. |
| `trace_writes.py` | Histogram of instructions writing into an address range (found the single `rep movsd` writer of Jizz samples). |
| `unixa.py` | iXalance `.ixa` unpacker: LZSS (10/4, MSB-first dwords) + RLE, DOS32 "Adam" header, relocation (`relocate`) → `exe.bin`, `image.bin`, `fixups.bin`, `reloc.bin`, `ip.txt`, `script.bin`. |
| `verify.py` | Byte compare of reference renders vs XM sample data (`jizz` by name, `stash` by XM order + drum copies); exit 1 on mismatch. |
| `xminfo.py` | XM parser (`parse`: header, patterns, instruments, sample headers/offsets; `pcm`: delta decode) + CLI listing. |
| `xmnotes.py` | Notes and retrigger gaps used per instrument in the patterns (derives playback note C#5 etc.). |
