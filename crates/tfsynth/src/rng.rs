//! The engine's random source: xorshift32 with a fixed seed, so renders are reproducible (Tunefish itself seeds
//! from system entropy, so its own renders differ run to run).

use synth_core::rng::Rng;

pub const SEED: u32 = 0x2545_F491;

#[derive(Clone, Debug)]
pub struct Xorshift(u32);

impl Xorshift {
    pub fn new() -> Self {
        Self::with_seed(SEED)
    }

    /// Seed 0 (a fixed point of xorshift) is replaced by [`SEED`].
    pub fn with_seed(seed: u32) -> Self {
        Xorshift(if seed == 0 { SEED } else { seed })
    }

    #[inline]
    fn next_u32(&mut self) -> u32 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.0 = x;
        x
    }

    /// Uniform in `[lo, hi)`.
    #[inline]
    pub fn range(&mut self, lo: f32, hi: f32) -> f32 {
        self.next_f32() * (hi - lo) + lo
    }

    /// Uniform integer in `0..n` (n > 0).
    #[inline]
    pub fn below(&mut self, n: u32) -> u32 {
        self.next_u32() % n
    }
}

impl Default for Xorshift {
    fn default() -> Self {
        Self::new()
    }
}

impl Rng for Xorshift {
    /// Uniform in `[0, 1)` (24 random bits).
    #[inline]
    fn next_f32(&mut self) -> f32 {
        (self.next_u32() >> 8) as f32 * (1.0 / 16_777_216.0)
    }
}
