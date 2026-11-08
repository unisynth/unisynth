# src/tests_synth.rs (sidecar: full detail of the AGENTS.md row)

Pull-only (not auto-injected).

- Synthetic only (10 tests), scheduler with a recording dummy voice: offsets + sub-block splitting, zero-first output, delays persisting across calls, lowest-free voice / no stealing, self-freeing voice finishes its sub-block, slot-order firing + overtake by a freed slot, 256-slot capacity, note/offset rejection, note-off releases every matching voice only, length panic.
