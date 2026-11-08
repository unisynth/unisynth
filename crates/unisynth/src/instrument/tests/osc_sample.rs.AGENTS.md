# src/instrument/tests/osc_sample.rs (sidecar: full detail of the AGENTS.md row)

Pull-only (not auto-injected).

- Sampler: unity pitch at note 60, pitch/coarse/fine/octave/ratio/pitch route/fixed Hz against an affine ramp with fixed gain `K`, rate independence, nearest vs linear, repeat/ping-pong/reverse/start against an independent walk of the original rules, loop bound clamps, linear wrap at the loop seam, no bank/index 0/bad bank, GSM decode + entry choice + decode-once pool sizing, one-shot (ignores note-off, ends with the sample, released by all-notes-off), PM send, call-split/reset independence, modulated start.
- Builds a tiny DLS bank in the test.
