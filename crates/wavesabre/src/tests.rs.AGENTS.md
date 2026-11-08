# src/tests.rs (sidecar: full detail of the AGENTS.md row)

Pull-only (not auto-injected). Unit tests, `#[cfg(test)] mod tests` in `lib.rs`.

- `harness` (`pub(crate)`): `run`/`run_at` (tempo) drive a `Device` over L/R (+ optional sidechain), `sine`, `peak_db`, `chunk`.
- Helpers: FastSin bit-model vs sin, random law vs reference formula, square partials, parameter mappings.
- `effect_tests_1..3`: kind ids + param counts, short/tiny chunks, Screwdriver vs C++ reference
  (`src/testdata/screwdriver_ref.txt`, ≤ 2 ulp, powf differs), Crusher step + hold, Scissor shapers + oversampling,
  Leveller peak/cuts (+ quirk), Echo taps vs tempo + worst case fits pool, Chamber arrival per mode + dry path (¼ per channel),
  Smasher threshold after lookahead + sidechain key, reset restores fresh state, Cathedral first arrival / freeze / pre-delay,
  Twister sweep range 2..134 + fractional + modes/vibrato finite.
- `synth_tests`: envelope timing in ms + curves, SVF low-pass response.
- `instrument_tests`: Slaughter/Falcon pitch + spectra (saw, pulse 50 % no even harmonics, FM sidebands), rng draws
  (Slaughter 768 phases at load, Falcon 1 per note-on), release ends voice, mono legato slide, main overwritten + param counts.
- `sampler_tests`: unity pitch plays every sample then stops, octave up skips + interpolates, repeat seam, ping-pong,
  manual boundary / reverse / start, empty source silent + inactive.
- `sample_instrument_tests`: Thunder/Specimen on the libgsm vectors (incl. uncompressed cap, octave halves length, no
  sample silent, malformed chunk error); Adultery without bank silent, Windows wave order vs upstream 0x44602 walk
  (needs `GM_DLS`), Adultery plays bank wave 0 (macOS bank default).
- `real_songs_load_with_all_devices`: env `WS_SONG_DIR` (52 reference songs) + optional `GM_DLS`; load, render, seek, finite;
  ≤ 1 silent allowed (SabreWulf_13, silent upstream too). Returns early when the env var is unset.
