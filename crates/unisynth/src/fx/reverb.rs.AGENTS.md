# src/fx/reverb.rs (sidecar: full detail of the AGENTS.md row)

Pull-only (not auto-injected).

- `Combs` core (Freeverb comb/allpass network) shared by freeverb and compact: 8 combs + 4 allpasses per channel, stock 44.1 kHz tunings scaled to the rate, optional comb spread.
- Sleeps after `SLEEP` = 8192 silent frames (input and output under 1e-7), clears its lines, wakes with the input;
- `SLEEP` `pub(super)`.

## History

- See change: compact-cauldron-xm-import
