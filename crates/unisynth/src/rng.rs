//! The instrument's random source: xorshift32. Each instrument seeds one from a single draw of the song random source
//! at load, and every voice derives its own stream from it at note-on, so no voice's randomness depends on other
//! tracks' render order or the player's sub-block size.

use synth_core::rng::Rng;

const FALLBACK: u32 = 0x2545_F491;

#[derive(Clone, Copy, Debug)]
pub struct Xorshift(u32);

impl Xorshift {
    /// Seed 0 (a fixed point of xorshift32) is replaced by a constant.
    pub fn new(seed: u32) -> Self {
        Xorshift(if seed == 0 { FALLBACK } else { seed })
    }

    /// A seed from a draw of any `Rng`.
    pub fn seed_from(r: &mut dyn Rng) -> u32 {
        (r.next_f32() * 4_294_967_040.0) as u32 | 1
    }

    #[inline]
    pub fn next_u32(&mut self) -> u32 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.0 = x;
        x
    }
}

impl Rng for Xorshift {
    /// Uniform in `[0, 1)` (24 random bits).
    #[inline]
    fn next_f32(&mut self) -> f32 {
        (self.next_u32() >> 8) as f32 * (1.0 / 16_777_216.0)
    }
}
