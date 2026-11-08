# src/instrument/tests.rs (sidecar: full detail of the AGENTS.md row)

Pull-only (not auto-injected).

- Onset frame, capacity/stealing/ages, unison, mono slide + glide, pitch/rate, velocity, pan, structural params, registry, call-split and sub-block independence (also through `Player`), seek/reset, envelopes (release end, env1 at output vs routed, rate independence, laws), LFO sync/free, vibrato, matrix sources, own-amount modulation; clocked: legato keeps phase and rising filter, transpose/bend resample the phrase;
- `an_instrument_is_quiet_only_without_voices_or_queued_notes`.
- Uses the real phase-mod operator on slot 1.
- Patches built in the old layout (`pm` = `params::old`) and encoded through `migrate::values` (`encode`);
- `v3`/`v3i` for `set_param`, automation, `PARAMS` lookups.

## History

- See change: add-cauldron-xm-import, add-quiet-track-skip, unify-v2-algorithms 6.5–6.8
