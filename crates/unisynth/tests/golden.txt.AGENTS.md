# tests/golden.txt (sidecar: full detail of the AGENTS.md row)

Pull-only (not auto-injected).

- Self-golden table of `src/instrument/tests/golden.rs`: `<patch> <FNV-1a 64 of f32 stereo> peak=<x>` per synthetic patch.
- Test fails on extra/missing lines.
- `UPDATE_GOLDEN=1 cargo test -p unisynth golden` rewrites it (review the diff).
