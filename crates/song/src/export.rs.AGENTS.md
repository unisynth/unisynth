# crates/song/src/export.rs (sidecar: full detail of the AGENTS.md row)

Pull-only (not auto-injected).

## API
- `tick_to_sample(song, tick)` = `round(tick·60·rate/(tempo·960))` in f64; `sample_to_tick(song, sample)` exact f64.
- `chunk(device)`: kind 64 → `unisynth::chunk::encode_text` (canonical chunk; `speech` joined by NUL, version 2 only when the text is non-empty; samples base64-decoded); effects → `fx::encode([mode, every parameter])`, ends in the `fx::TAG` version tag (no imported trailer).
- `flatten(song, track)` → `(start, end, pitch, vel)` ticks: placements sorted by start, loops repeat every clip length, notes cut at the placement end. Saturating arithmetic (hostile ticks never overflow).
- `controller(song, track, pick, rest)` → `Option<Vec<Point>>`: clip `wheel`/`bend` envelope per placement and per loop pass (start/end values, stable step order), back to `rest` at the placement end; `None` when no clip of the track has one.
- `export(song)` = `export_model(song)?.write()`; `export_model` → `BlobM` (the live host builds its schedules from it).

## Behaviour
- `events`: one open note per pitch; earlier note cut at the next start, superseded (same sample) dropped, off ≥ on+1; at one sample note-offs first, then note-ons in pitch order.
- `export_model` validates first (any issue → `Err`). Tracks in song order, master last; one lane per track; bypassed devices and their lanes left out; muted track = volume 0 + no events; blob tempo = `round(tempo)`; header length = `(frames+0.5)/rate`.
- Lanes → `AutoM` (tick → sample, value·255 rounded). Instrument clip envelopes are added as `ctl.wheel` / `ctl.bend` automation of the first unbypassed instrument (rest = `params::DEFAULTS[ctl::WHEEL/BEND]`).
- Receives write `recv::encode(send, channel.to())`.

See change: add-v2-import, unify-v2-algorithms.
