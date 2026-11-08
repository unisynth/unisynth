# src/preflight.rs (sidecar: full detail of the AGENTS.md row)

Pull-only (not auto-injected).

- Song judge.
- `RATE` 44100, `BLOCK` 128, `MAX_DEVICES` 256, `MAX_AUTOMATED` 64, `MARGIN` 1e-5, `param_count(kind)`, `chunk_params(kind, chunk)` (exactly `4+4n` bytes, trailing u32 = length, settings checked), `check_params(kind, &Params)` (`Params = [f32; MAX_PARAMS]`, `MAX_PARAMS` 26: ID 14 / 28) (range, then the device's own `check`), `preflight(song, env)` (strict: every device must be an owned kind, used by `GlorpPlayer`), `preflight_owned(song, env)` (skips unowned kinds, lanes on them, and rate / `MAX_DEVICES` of a song without owned devices; the `Kinds` hook), `Reject{device,kind,param,reason}` (`load_error()` → coarse `LoadError`), `Reason::{Rate,UnsupportedKind,ChunkLength,ChunkSize,Range,Setting(text),LaneDevice,LaneParam,Automation(text),TooManyAutomated,TooManyDevices,Load}`.
- Automation = **box per device**: range of chunk value + every lane point per parameter (≤ 64 devices, stack table), widened by `MARGIN`, every vector inside must be supported whatever the write order / interpolation:  ID 21 is judged on the corner with P4 and P6 at their top (partial count ≤ 512); ID 22 on the corner with P2 at its bottom and P4 at its top (weight sum, `Error::Weights` → `Setting`/`Automation("3D map weight sum overflows (small P2 with large P4)")`, param 2); ID 35 / ID 38 probe each automated parameter at both ends through `check` (ID 38 P12 must be exactly 0.5: any lane on it is refused); ID 32 judges the corner with P2, P4 at their top and refuses a P7 lane that crosses 0.5 (stale glide after B → A).
- Note lanes are judged for ID 20 only: every track that carries an ID 20 is scanned and the song refused (`Setting("note velocity above 213 …")`, names the ID 20 device) when its lane holds a note-on above `id20::MAX_VELOCITY` (the format carries 0..=255); the song format cannot carry a note > 127 (note-off = bit 7 + low 7 bits) and every other instrument ignores velocity.
- IDs 3/7/14/20/25/27/28/34 take any parameter lane (no `judge` arm: every vector in [0,1]^n is valid, such as ID 20 [0,1]^16); ID 30 is judged on the corner with P16 and P20 at their tops (cascade topology).
- A track lane index outside the song is not judged here (the velocity scan skips it): `Song::parse` already refuses it (`BadIndex`, -1 included) and the core `Player::new` refuses a `Song::from_sections` song with `BadIndex`.
- `MAX_DEVICES` / `MAX_AUTOMATED` are this crate's caps (no extra pool cap: the caller sizes the pool via `required_pool`).
- One pass over the song, no heap.
- Lane mask is a u32.
- See change: id28 implementation.
