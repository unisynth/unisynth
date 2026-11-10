# tools/xm-gen

Generator maps of the 1998 softsynth intros (change add-plain-xm-import, design D6). Python; unicorn + capstone in
`out/venv` (created by `run.sh`, git-ignored). Material (XMs, the executables from the scene.org archives) comes from
`$ZENEKK_DIR` (default `~/Downloads/zenekk`) and is never committed; only `maps/` is. Addresses assume the unpacked
image's object 1 at 0x10000.

| File | Purpose |
|---|---|
| `.gitignore` | Ignores `out/` (venv) and `__pycache__/`. |
| `emulate.py` | `Emu(image, base)`: flat image + 64 MB heap, DPMI 0x501 allocation (other DPMI/DOS ints succeed), `poke`, `seed` (srand words), `alloc_table` (free slots −1), `who(start, length)` (EIP of each byte's last writer, range hook), `on(addr, fn)`, `run(start, stop)`: FPU control word 0x1F7F stub, then ONE `emu_start(timeout=0)` (slicing corrupts x87 state). |
| `extract.py` | `INTROS` (per intro: exe, XM, start/stop of the sample calculation, module address, pokes/seed/alloc table, stored-writer EIPs, Fudge's generator entries → engine ids `fm-shaper` 0x2cfaa / `sweep-drum` 0x2cdd7). `locate` (exact bytes, 64-byte mid probe, else between located neighbours). Per XM sample: majority writer in `stored` → stored; else generated: Fudge `params {frames, args: 9 raw u32 pushes, carry (drum: dword at esp−0x20 at entry)}` matched by destination offset, Quad `poespack` `{slot}` (generation order; parameters arrive with add-poespack-softsynth). Writes `maps/<intro>.json` one entry per line. Cross-checked: Fudge args = the verified port's 19 distinct records. |
| `le_unupx.py` | `unpack(exe_bytes, log) -> (image, {base, oep, size, used})`: UPX 0.20 LE (DOS/4GW or PMODE/W) loaded flat, fixups applied, stub emulated to the OEP; CLI `<exe> <out.bin>` (+ `.meta`). |
| `maps/fudge-asc.json` | Generated (`extract.py`): 25 samples, stored 7, 14, 15; 22 generated (11 `fm-shaper`, 11 `sweep-drum`; 18/19/24 share 11/13/11's record). Do not edit. |
| `maps/quad-pure.json` | Generated: 34 samples, stored 33, 34, 35; 31 `poespack` slots. Do not edit. |
| `maps/quad-theta.json` | Generated: 27 samples, stored 1, 2, 3, 18; 23 `poespack` slots (6 and 8 located between neighbours: the XM is a later build). Do not edit. |
| `run.sh` | Creates the venv when absent, runs `extract.py "$@"`. |
| `xmsamples.py` | `xm_samples(path)` → `[(instrument 1-based, 'b'\|'w', pcm)]` per sample in file order (delta-decoded; 16-bit as ints). |
