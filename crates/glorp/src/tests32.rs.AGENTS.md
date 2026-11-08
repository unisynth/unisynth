# src/tests32.rs (sidecar: full detail of the AGENTS.md row)

Pull-only (not auto-injected).

- Synthetic only, ID 32 (every body runs on a 64 MiB thread): rejection table + atomicity, mode-A delay-bound boundaries (P2 0.49/0.50, depth 0.40/0.41), `process` refusing a stale long smoothed delay without touching buffers/state, P10 bypass (identity + state frozen), mix 0 = exact dry, block-split bit identity, reference f32 constants, impulse echo at the glided base delay (88.2 samples, area 2.25), independent history-based f64 model of both modes (f32 phases / base-delay glide / table sine kept: LFO phases and the glide stall are f32 state; A ≤ 1e-4, B ≤ 5e-4) incl. per-block automation, mode switches, voice-count changes with frozen-line resume; `set_param` order hazard (B→A with long P2 rejected until P2/P4 lowered, raise P2+P4 in A, A→B needs P7 first).
