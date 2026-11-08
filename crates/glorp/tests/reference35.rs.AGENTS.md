# tests/reference35.rs (sidecar: full detail of the AGENTS.md row)

Pull-only (not auto-injected).

- Env-gated (`GLORP_REF_ROOT`): song device 27 replay (600 blocks from block 7539, all recorded `SetParam` through `set_param`, silent blocks before the window skipped: zero state + zero input = zero), tolerance 4e-3, measured max 3.170e-3; 8 synthetic vectors `id35vec/vec01..08` (the original's output captured by the harness with `GLORP_OUT_TAP`: noise/tones/impulses, 1..256 stages, extreme params, P5 = 0.667 boundary, automation, stage-count jumps, the song's own automation on noise): bit-exact except vec05 (1.454e-5) and vec08 (2.007e-4); asserts the original leaves the aux pair untouched; automation-log sanity; negative controls via `catch_unwind` (flipped reference bit must panic with the `got … want` mismatch message; P0/P1/P2/P3/P4 nudges all leave tolerance).
- `GLORP_REF_ROOT` unset => explicit skip message; set but root/`389811-2.bin`/`ev_full.csv`/`tapspec.txt` missing => fail loudly.
