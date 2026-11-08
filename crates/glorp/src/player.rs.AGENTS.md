# src/player.rs (sidecar: full detail of the AGENTS.md row)

Pull-only (not auto-injected).

- `GlorpPlayer<'a>` owned API: `required_pool(bytes)` (caller must impose its own pool-byte limit; no cap here), `new(bytes, pool)` (parse → strict `preflight` with `Reject` diagnostics → `Player::new(.., NoRng, 128)`), `render_f32` (`Err(fault)` + zeroed buffer on a device fault), `seek`, `length_frames`, `fault`, `player()`.
- `NoRng` (devices never draw).
