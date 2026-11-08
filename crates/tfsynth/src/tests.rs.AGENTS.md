# src/tests.rs (sidecar: full detail of the AGENTS.md row)

Pull-only (not auto-injected). Unit tests, `#[cfg(test)] mod tests` in `lib.rs`; one `mod` per area.

- `params_tests`: layout + names, mapping spot values, byte quantisation. `rng_tests`: determinism + range, seeds differ.
- `table_tests`: table shapes; `ifft512` single bin = sinusoid, linear + unscaled.
- `wavetable_tests`: even bins at scale 1; width/gain/cutoff; band limit zeroes upper bins + `band_bins` heuristic;
  rebuild only on change; phase-stage quirk range + drift; rendered table peaks at even bins (FFT); normalised + DC-free.
- `harness` helpers: instrument builder, `sine()`, `render`, Goertzel `mag`/`peak`, `rms`.
- `voice_tests`: pitch + octave, fixed freq ignores note, detune L/R, linear-balance pan, velocity + unison sum,
  drive clips to unit gain, first sample in range, slop + glide, organ gate without envelope.
- `note_tests`: steal oldest, stolen note-off, polyphony limit, linear-Hz bend on all voices, mod wheel source, bad patch length.
- `determinism_tests`: same patch + notes = bit-identical render.
- `env_lfo_tests`: attack times, linear decay/release targets, decay rises when modulated > 1, sustain chases a changed
  level, LFO shapes + rate.
- `modmx_tests`: slots 9/10 inert, products per target, self-mod takes one block, routed envelope releases instead of gating.
- `filter_tests`: Rbj HP/BP vs analytic |H|, notch cut + 1-sample latency, ladder LP slopes, filters off = bypass,
  noise uniform + decorrelated + band-passable.
- `fx_tests` (cfg `tf-fx-all`): reserved kinds pass through, chain need + out-of-pool slot acts as none, auto-sleep after
  2 s + wake on note, distortion curve, delay taps/gains, reverb arrivals/spread/room, chorus flat spot + 10 h wrap,
  flanger sweep 0.126/0.124 Hz + lfo-0 L = R bounded past a fold, formant F1/F2 per vowel, EQ cuts/boosts.
- `disabled_kind_acts_as_none` (cfg without `tf-fx-reverb`, with `tf-fx-delay`): reverb slot acts as none, later delay slot still processes.
