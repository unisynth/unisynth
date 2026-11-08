# src/instrument/tests/pool.rs (sidecar: full detail of the AGENTS.md row)

Pull-only (not auto-injected).

- Memory: `need` is a safe bound (loads in exactly `need` bytes at 16 start alignments), loading uses ≤ need, one byte under the consumed amount → `OutOfMemory`, PM-only < 1/5 of wavetable (difference = one `Wt` per voice), shared GSM decode, hostile sample sizes → `BadChunk`, structural params automated to larger memory change nothing.
