# src/modmx/tests.rs (sidecar: full detail of the AGENTS.md row)

Pull-only (not auto-injected).

- Neutral matrix, Scale product, Offset sum, source values, LFO views, slot-amount modulation one pass late, same-pass LFO depth, target table, table = scanning.
- Add: sequential clamp (0.75+0.5→1, −0.125 → 0.875), Scale/Offset blind to Adds, channel vs voice targets, CC7 and V2-note sources, Add on legacy target / Scale on parameter target do not route;
- `a_slot_past_ten_needs_the_add_module`, `a_lane_that_can_make_a_slot_add_needs_the_add_module` (gated `us-matrix-add`).

## History

- See change: add-v2-import
