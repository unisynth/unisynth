# crates/glorp overview (sidecar: detail moved out of the AGENTS.md intro)

Pull-only (not auto-injected). Flow diagram + automation-box rules: `docs/synth/glorp.md`; device research: `research/glorp-devices.md`.

- Scope: MIT `no_std` f32 devices compatible with the proprietary Glorp song's own device set; independent behavioural implementation
  written from observed behaviour; no original binary or pseudocode is included. The original has no licence, so no song, dump,
  recorded tap or proprietary vector enters git.
- Devices: ID 30 granular delay / reverb multi-effect (cascade topology 0 only), the song-used subset of ID 38, the complete ID 34
  frequency shifter, the second-order subset of the ID 35 all-pass cascade, ID 32 (chorus / all-pass-feedback ensemble, both modes,
  buffer-bounded), the complete ID 7 (stereo look-ahead compressor / limiter, internal or side-chain detector), the complete ID 3
  (stereo waveshaper / saturator), ID 21 additive recursive-sine-bank instrument (≤ 512 partials per voice), ID 22 circle-oscillator-
  through-3D-trig-map instrument (poly, weight-overflow guarded), ID 20 negative-feedback comb resonator (velocity sensitive, ceiling 213),
  ID 25 percussion synth, ID 27 comb-resonator percussion synth, ID 14 modal resonator instrument (poly or mono), ID 28 unison
  oscillator-bank instrument. Instruments share the 16-voice scheduler (`synth`).
- Adapter: `kinds::GlorpDevice` (`Device` + `Kinds`) behind a song preflight (`preflight::preflight`, run by `Kinds::preflight` inside
  `load_devices`, so `Player::new` cannot skip it) and the owned `player::GlorpPlayer` (full `Reject` diagnostics).
  `Device::set_param` returns `()`, so every unsupported id / chunk / setting / rate / block size and every automation lane that could
  leave the supported settings is refused **before** a device exists; a `Shell` fault latch (not forwarded, silence from then on)
  covers a `Device` driven by hand.
- Limits: sub-block 128 (`Kinds::block`, per song) and 44.1 kHz only; unison cannot be represented (see the ID 25 / 27 sidecars).
- Whole song: the real song is accepted and plays (every id implemented, `GlorpPlayer`); whole-song render vs the original's 16-bit
  render: tolerance parity of the mix (see `docs/synth/glorp.md` §Whole song).
- No `synth-mix` kind id is assigned (ids 3 and 7 are WaveSabre's): do not put `GlorpDevice` in an `Either` with `WsDevice`.
- In root `default-members`.
