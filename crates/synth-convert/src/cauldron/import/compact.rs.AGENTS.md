# src/cauldron/import/compact.rs (sidecar: full detail of the AGENTS.md row)

Pull-only (not auto-injected). Compact layout of a converted mini-cauldron XM. Tests inline (`mod tests`, 13). See change: compact-cauldron-xm-import, slim-cauldron-chain.

## Layers
- `layers(&mut Replay, end, rate)` → `Layers {keep, same_tick, echoes, absorbed}`: in time order drops a trigger repeating a kept one (same instrument, sample, note, offset; other channel) at the same frame, or later before the kept one's `end` within `ECHO_WINDOW_S` 2.5 s at a lower first-tick volume.
- Kept trigger absorbs same-frame copies per side: same-pitch copies add amplitudes, detuned copies add powers; sum sets its pan (equal-power) and scales its volume on every tick.
- `Absorbed {pair, taps: Vec<Tap {delay s, pan, gain}>}` per kept trigger.

## Packing
- `pack(r, ids, decim, span)` → `TrackPlan`s: first-fit per record across ratios (no overlap, sounding range ≤ `SEGMENT_SPAN`), class = most frequent `class_of` (tie → nearest 0) clamped into reach and `MIN_CLASS..MAX_CLASS`.
- `pack_by(r, key, span)`: first-fit per key whatever the channel (stored samples).

## Ambience
- `ambience(&[&Absorbed])` → `Ambience {echo: Option<Echo {left, right, gain}>, reverb}`: none unless ≥ half the triggers had layers; reverb then; echo = most common (first left tap, first right tap (missing side takes the other's time), RMS gain), times to the ms, level to 1/64, when ≥ half the triggers had taps.
- `Ambience::devices(tempo)` → (tempo delay without feedback, wet gain/(1+gain), + freeverb `REVERB` [0, 0.3, 0.5, 1, 0.0807, 0.591, 0.12, 0.02]; track-volume make-up). `longest_ms` = longest tap.

## Chain slimming
- `OPEN_HZ` 16 kHz, `CLIP_SLACK` 1e-4 (−80 dB).
- `slim_lowpass(chain, hz, slides)`: one stage; none at ≥ `OPEN_HZ` without a slide; latency follows.
- `clips(chain)` → (track device index, threshold); `input_peak` = solo render of devices `..upto` at volume 1; `idle_clips` = clips whose input ≤ threshold + slack (no-op), checked in chain order; `remove_device` removes a device and shifts later automations.
