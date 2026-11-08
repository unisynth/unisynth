# examples/render.rs (sidecar: full detail of the AGENTS.md row)

Pull-only (not auto-injected).

- (`required-features = std`) `render <song.bin> [out.wav] [bank.dls]`: loads a unified song with the modules this build has, renders, prints peak, pool size and `rendered in … s` (for the parity tool); exit 1 naming the missing module, 2 usage, 3 silent.
- Used by `tools/check-song-features.sh`.
- `UNISYNTH_SEED` varies the random draws;
- `US_NO_SKIP=1` renders without the quiet-track skip (A/B).

## History

- See change: add-quiet-track-skip
