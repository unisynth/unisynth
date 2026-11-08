# src/modmx.rs (sidecar: full detail of the AGENTS.md row)

Pull-only (not auto-injected).

- Slots: `SLOTS` = 16 with `us-matrix-add` else `SLOTS_BASE` = 10 (`pm::MOD_SLOTS1`, the chunk's one-byte block; slots 11–16
  need the module). `OWN_SLOTS` = 12: slots whose own amount is a target (`target::SLOT`..).
- Operations (`op::{SCALE,OFFSET,ADD}`, `Choice(3)`, `COUNT_V1` 2): Scale slot = factor, factors on one target multiply
  (`scale(t)`); Offset slot = addend in the target's unit, addends sum (`offset(t)`); Add slot (V2) adds `amount·source` to
  a parameter target's normalized value, clamped to 0..1 after each slot in order (`Matrix::apply(v, channel)`, V2
  `storeV2Values`). Slot result = `amount · source · own` (own = Scale slots aimed at the slot's amount, one pass late).
- Sources (`source::*`, `Choice(17)`, `COUNT_V1` 10): none, env 1–4, LFO 1–2, wheel, velocity, key, CC2..7 (`CC2` = 10..15),
  `NOTE_CENTRED` = 16 (V2 note: `2·(note − 48)/128`); `decode`/`encode` (expanded choice). `Sources` (`env`, `lfo_shape`,
  `lfo_add`, `wheel`, `velocity`, `key`, `cc`, `note`). LFO sources read with depth modulated by what the matrix says so far.
- Targets (`target::*`, 0..`COUNT`): voice/osc/noise/filter/env/LFO-depth/slot-amount targets frozen with chunk v1 (below
  `PARAM` = 78), parameter targets `PARAM`..`COUNT` = 142 via `PARAMS` (64 V2 `isdest` params in `sounddef.h` order, by v3
  names); `param(t)`, `is_channel(t)` (= `params::is_strip`), `scales_physical(t)`, `offset_range(t)`, host `name(t)`.
- `routes(p, i)`: slot has a source and a valid target; Add only on parameter targets, others only below `PARAM`; Add is off
  without `us-matrix-add` (`routes_as`). `needs_add(p, i)`: slot 11–16, Add, or a source ≥ `COUNT_V1` (reads Add always).
  `adds_to(p, slot, param)` (host): slot is an Add route to `param`.
- `Query` (scale/offset) implemented by `Matrix` (scan) and `Tab` (`Matrix::tab()`: every target's product/sum in one pass,
  bit-equal; sized `PARAM`, skips Adds). `has_adds`, `apply_from`.
- Without `us-matrix`: `eval` returns at once, `Matrix::scale/offset` and `Tab` queries read 1/0.
- See change: add-unified-synth, shrink-unified-synth, add-v2-import (5.2), unify-v2-algorithms 6.5–6.8.

## Former AGENTS.md row (moved, full text)

- Mod matrix: `Matrix` (`eval`, `scale(t)`, `offset(t)`, `apply` for Add), `source::*`, `op::*`, `target::*`, `routes`/`needs_add`, `Query` with `Matrix` (scan) and `Tab` (one pass).
- 16 slots with `us-matrix-add` else 10; no-op without `us-matrix`.
