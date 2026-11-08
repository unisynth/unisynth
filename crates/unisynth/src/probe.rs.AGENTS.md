# src/probe.rs (sidecar: full detail of the AGENTS.md row)

Pull-only (not auto-injected).

- Feature `probe` (host only): `VoiceSnap` (id = note-on seq, note, amp, released, age_s, env (level, `Stage`)×4, LFO phase×2, sampler pos×3, cutoff (norm, offset)×4), `Probe { voices, taps }`, `cutoff_hz`, `Ring` (`TAP_LEN` 2048 power of two;
- `begin(n)` clears, `add(frame, slot, in, out)`, `end`, `read(slot)` oldest first).
- Read only: goldens equal with taps on.

## History

- See change: add-device-live-view
