# src/tests7.rs (sidecar: full detail of the AGENTS.md row)

Pull-only (not auto-injected).

- Synthetic only, ID 7: rejection table + atomicity, P0 boundary 0.4999/0.5 and call-form refusal (`Sidechain`, buffers/state untouched), exact 87-sample impulse delay, pure delay with unity gains, gain laws (6 per unit), steady-state hard-knee law, independent f64 model (6 param sets, 2e-5; linear attack/release ramps, 1e-6), max-of-channels shared gain, side-chain drives gain / aux never written / quiet aux = no compression, P0 switch keeps envelope + ring, block-split bit identity, `set_param` keeps state, silence/release recovery, huge-input finiteness, NaN detector forgotten after one sample (pinned from the original), length panics (`should_panic(expected)`), negative control: 1/255 nudges of P1..P6 leave the model tolerance.
