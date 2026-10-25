//! Random source passed explicitly in the render context. Each engine supplies its own law and seed
//! (no global state), so renders are deterministic.

pub trait Rng {
    /// Next value in the engine's own range.
    fn next_f32(&mut self) -> f32;
}
