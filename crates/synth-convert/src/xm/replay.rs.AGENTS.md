# src/xm/replay.rs (sidecar: full detail of the AGENTS.md row)

Pull-only (not auto-injected). FT2 tick replay. See change: add-cauldron-xm-import, add-cauldron-control-rate, add-plain-xm-import.

- `replay(&Xm, rate) -> Result<Replay, String>`; `Replay {triggers, length}`; `Trigger {channel, tick, frame, at, instrument, sample, note, offset, ticks: Vec<TickCtl>}`; `TickCtl {row, at, frame, frames, freq Hz, vol 0–1, pan 0–1}`; `row` = first tick of a row (first `EEx` repeat).
- Tick = `trunc(rate·2.5/bpm)` frames (openmpt). Linear periods `7680 − 64·(note − 1 + rel) − fine/2`, `freq = 8363·2^((4608 − p)/768)`; slid periods clamp to FT2 1–31999 (`PERIOD_MIN`/`PERIOD_MAX`). Volume `vol·env·fade·global`; envelopes with sustain/loop, fadeout after key-off, key-off without volume envelope cuts. Pan is `pan/256` (128 = centre, as openmpt).
- Effects 0–4, 8, 9, A, C, D, F, G, H, P, R, E1, E2, E9, EC, ED, EE, X1, X2 + volume column set vol (0x10–0x50), fine slide down/up (0x80–0x9F), set pan (0xC0–0xCF); others → error naming pattern/row/channel (letters past F: `effect K`, `effect X3`). `F00` ignored (openmpt).
- `Hxy`/`Pxy`: ticks after the first, x up/right else y down/left, memories per channel; `G`/`H` change `global` when their channel is processed (FT2: all channels that tick; equal on channel 0).
- `X1x`/`X2x`: tick 0, period ∓ x, memories per direction.
- `Rxy`: memories x, y; counter on every tick except a note's tick 0 (reset), across rows; at y: `retrig_volume` (FT2 table), then the row's set-volume column again, new trigger (envelopes, fade, auto-vibrato restart).
- Auto-vibrato (`auto_vibrato`, `av_start`): per tick incl. 0, amplitude += `(depth<<8)/sweep` while key down (cap depth<<8), position += rate, period += `wave·amp/16384` truncated toward 0; waves: `av_sine` = −round(64·sin), square ±64 (pos > 127 → +64), ramps. Pinned to `fixtures.rs`.
- `3xx` with another instrument number keeps the sounding instrument (FT2); the number resets volume + envelopes only.
- A trigger ends at the channel's next trigger, a cut, a finished fadeout or the song end.
