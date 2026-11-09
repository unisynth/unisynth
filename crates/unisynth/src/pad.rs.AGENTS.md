# src/pad.rs (sidecar: full detail of the AGENTS.md row)

Pull-only (not auto-injected).

- PAD table (`us-osc-pad`, add-tf3-import; TF3 spec §3.2): `Shape`, `build(shape, seed, table)` (partials → per-bin magnitude × profile, counter-hash phases, Hermitian part → one 2^15-point complex FFT in place = real part of the 2^16-point inverse DFT, × 1/(√2·peak); no scratch memory), `sample` (index round(phase·2^16), phase += inc/260 → 0.499·f, drive, clip), `play` (through `Pts::decim`), `LEN`, `F0`, profiles `SINGLE/DETUNE/SPREAD/GAUSS`. Tests: packed transform = direct real part, normalisation, partial laws, octave-low pitch; opt-in `parity` (magnitude spectrum vs `pad-ref`, 16-bin bands within 2 %).
- See change: add-tf3-import (tasks 3.1–3.3).
