# src/xm/replay.rs (sidecar: full detail of the AGENTS.md row)

Pull-only (not auto-injected). FT2 tick replay. See change: add-cauldron-xm-import, add-cauldron-control-rate.

- `replay(&Xm, rate) -> Result<Replay, String>`; `Replay {triggers, length}`; `Trigger {channel, tick, frame, at, instrument, sample, note, offset, ticks: Vec<TickCtl>}`; `TickCtl {row, at, frame, frames, freq Hz, vol 0–1, pan 0–1}`; `row` = first tick of a row (first `EEx` repeat).
- Tick = `trunc(rate·2.5/bpm)` frames (openmpt). Linear periods `7680 − 64·(note − 1 + rel) − fine/2`, `freq = 8363·2^((4608 − p)/768)`; slid periods clamp to FT2 1–31999 (`PERIOD_MIN`/`PERIOD_MAX`). Volume `vol·env·fade·global`; envelopes with sustain/loop, fadeout after key-off, key-off without volume envelope cuts. Pan is `pan/256` (128 = centre, as openmpt).
- Effects 0–4, 8, 9, A, C, D, F, G, E1, E2, E9, EC, ED, EE + volume column set vol/pan; others, F00 and auto-vibrato → error naming pattern/row/channel.
- `3xx` with another instrument number keeps the sounding instrument (FT2); the number resets volume + envelopes only.
- A trigger ends at the channel's next trigger, a cut, a finished fadeout or the song end.
