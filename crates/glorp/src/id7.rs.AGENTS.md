# src/id7.rs (sidecar: full detail of the AGENTS.md row)

Pull-only (not auto-injected).

- `Id7::new(&[f32;7])`, `set_param(i, v)` (atomic: `Err(Index)` i ≥ 7, `Err(Range)` non-finite / outside [0,1]), `sidechain()` (P0 ≥ 0.5), `process(l, r)` (internal detector) / `process_sidechain(l, r, sl, sr)` (aux pair read only) in place, `Result<(), Error>`: the wrong form for P0 = `Err(Sidechain)`, buffers + state untouched; unequal lengths panic (`equal length required`).
- Look-ahead compressor: both inputs × input gain `2^((2P1−1)·12/6)` into two 88-sample rings (output = input **87 samples later**), detector `max(|L·g|,|R·g|)`, linear-ramp envelope (attack `(1000/44100)/((5000P4²+1)·0.2)`, release `(1000/44100)/(5000P5²+1)` per sample, clamped to the detector, NaN detector → envelope NaN for one sample as the original's `jb`), hard-knee gain `(((e−thr)/ratio+thr)·mk)/e` when `e > thr` (`thr = 2^((P2−1)·6)`, `ratio = 2+18P3²`, `mk = 2^((2P6−1)·2)`) else `mk`; one shared gain for both channels.
- Params re-read per call, nothing ever reset.
- `2^e` = f64 `libm::pow` rounded to f32.
- Fixed 44.1 kHz.
- All 7 params fully modelled; nothing in range unsupported.
- **Bit-exact (max error 0) vs the original** on the song window (153,600 samples, 40,487 compressed) and 11 synthetic vectors (481,280 samples, incl.
- NaN/inf, side-chain, automation, extremes); only the 3 `powf` gain factors could differ by an ulp on another libm (test bound `1e-7+3e-7·|ref|`).
