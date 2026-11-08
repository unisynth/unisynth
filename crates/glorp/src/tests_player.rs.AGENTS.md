# src/tests_player.rs (sidecar: full detail of the AGENTS.md row)

Pull-only (not auto-injected).

- Synthetic (random-song soundness includes ID 22 with P2 / P4 lanes at the guard edges: accepted ⇒ every lane vector passes `check_params` and the render is finite), `GlorpPlayer`: four-device chain `[25, 38, 34, 3]` with four lanes and a three-track graph (ID 27 + ID 25 → master `[7 side-chain, 35, 32]`, receives ×0.8 / ×0.5, three lanes) **bit-identical** to the standalone devices driven by hand; seek == straight render (both songs); every kind alone (ID 20 makes sound at velocity 100); random-song soundness (accepted ⇒ every lane vector at every frame passes `check_params` and the render has no fault; generator not vacuous).
- Hand-driven model's `lane_at` uses `math::lerp` (the engine's lane form).
