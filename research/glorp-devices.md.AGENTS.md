# glorp-devices.md (sidecar: full detail of the AGENTS.md row)

Pull-only (not auto-injected).

## Scope
- Reverse analysis of the proprietary Glorp song (39 device instances, 14 ids). Original unpacked engine rendered in an isolated Wine harness (external, not in git).
- All 14 ids have a runtime implementation in `crates/glorp` (tap-checked); ID 30 is the last one (7 instances, `id30.rs`). Remaining unmodelled: ID 30 cascade topologies 1/2 (refused by the preflight), unison, dead reverb parts.
- Lane interpolation gap fixed (`Device::lane_lerp`); ID 21 device 5 whole-song parity through the player.
- Document holds: Status and evidence, Factory and dispatch, algorithm evidence, one section per implemented id, shared DSP anchors, isolated reference harness, player integration, §Performance and size, Continuation.

## Measured results per id (kept verbatim from the runs)
- ID 38 (LP/HP, post-dist 0–2, envelope 0 only).
- ID 34 frequency shifter: complete.
- ID 35 all-pass cascade, second-order topology; song window ≤ 3.2e-3; 6/8 synthetic vectors bit-exact. External math experiment: alt. coefficient math 61.298 % / 152 vs current 60.397 % / 136 full song; 83.469 % / 10 only with original dev 27 input = hypothetical oracle, provenance unclear, no change recommended alone. Corrected-WAV SNR ≈ 67.9 dB, 32 zero padding frames, no audibility claim.
- ID 32 chorus / all-pass-feedback ensemble: whole-song parity of devices 22/23/24 ≤ 5e-5; 10 synthetic vectors ≤ 3.4e-5.
- ID 7 look-ahead compressor / limiter with side-chain: complete; song window + 11 synthetic vectors bit-exact.
- ID 3 waveshaper / saturator: complete; whole-song tap of device 0 + 13 synthetic vectors ≤ 1.2e-7; 36 of 20.0 M samples not bit-exact (C-library `powf` ulp).
- ID 25 percussion synth + shared 16-voice scheduler: whole-song device 8 93.13 % exact, max 6.9e-4; 44 synthetic vectors worst 1.2e-2 (libm tolerance parity; exp-helper provenance note in §ID 25).
- ID 27 comb-resonator percussion synth (same scheduler): whole-song devices 9/10 85.1 % / 69.5 % exact, max 5.8e-4 / 4.6e-4; 41 synthetic vectors worst 9.0e-3 at peak 2.1; no song automation exists, note-off inaudible.
- ID 20 negative-feedback comb resonator: 16 params latched at note-on, velocity sensitive, refusal above 213; whole-song device 4 98.80 % exact, max 1.1e-4 at peak 4.7; 53 synthetic vectors bit-exact; level-follower voice end, group 3 / P4 inert (§ID 20).
- ID 22 circle oscillator through an iterated 3D trig map: whole-song device 7 99.9997 % bit-exact, max 1.2e-7 at peak 1.9; 56 synthetic vectors ≤ 9.6e-7. Original returns NaN for P2 = 0 with n ≥ 15: weight-overflow guard `Error::Weights` + preflight corner (§ID 22).
- ID 14 noise-excited modal resonator bank, poly + mono: whole-song device 3 92.13 % exact, max 1.25e-5 at peak 0.43; 97 synthetic vectors ≤ 3.3e-4 of peak (§ID 14).
- ID 28 unison oscillator bank through 4-pole ladder: implemented (§ID 28).
- ID 21: whole-song device 5 through the player 99.446 % exact.
- ID 30 granular delay + reverb + filters + shifter + all-pass cascade: whole-song devices 15/16/17/18/21 100 % bit-exact (device 18 inactive in song), device 19 75.4 % exact max 2.5e-13, device 20 62.9 % exact max 1.414e-4 at peak 1.47.

## Perf + size (external probes, not in git; §Performance and size)
- Whole-song host render: opt z 0.82–0.90× / opt 3 1.30–1.32× real time (i9-9900K, busy host, 2–3 runs); pool 31,057,326 B; ID 30 = 66.2 % of samples.
- i686 `no_std` probe: synth only 37,376 B squishy (42,496 with song); no visuals / audio output; `/STACK:0x10000000`.
- Wine exit code = checksum low byte only.
