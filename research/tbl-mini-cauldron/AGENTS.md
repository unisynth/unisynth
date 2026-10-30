# research/tbl-mini-cauldron

Reverse engineering of TBL's precalc sample synth "mini-cauldron" (Jizz v1, Stash v2, 1997). `out/` git-ignored (intro images, XMs, renders).

| File | Purpose |
|---|---|
| `.gitignore` | Ignores `out/` (copyrighted images/XMs/renders, venv) and `__pycache__/`. |
| `README.md` | Overview, result table (Jizz 32/32, Stash 66/66 bit-exact), reproduce via `tools/run_all.sh`, legal note. |
| `mini-cauldron-spec.md` | Implementation spec: system overview (XM template + precalc synth + XM replay), timing/units (R = 35 441.2 Hz, step 3530 frames = 1 row, f = pitch·R/65536), v1 record layout/state/driver/retrigger (compounding mul)/frame/spike wave/filter (damped mass-spring, fc ≈ √c·R/2π)/echo (3-step, wraps)/post, v2 deltas (f32 record, waves 0–5, hold/release gate, comb, doubler, decim 1/2/4, tail fade, specials, drum kit, self-modifying two calls, in-place delta stale bytes), x87 PC24 numeric model + Rust recipe, RNG, decoded patch tables, XM-side settings, port notes. |
