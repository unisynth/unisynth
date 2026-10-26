//! LFOs (design D5/D6): a phase in turns advanced by elapsed time, five shapes, two views of the output.

use crate::tables::sin_turns;

/// Shapes (`choice(p, 5)` of `l::SHAPE`).
pub mod shape {
    pub const SINE: usize = 0;
    pub const RAMP_UP: usize = 1;
    pub const RAMP_DOWN: usize = 2;
    pub const SQUARE: usize = 3;
    pub const RANDOM: usize = 4;
    pub const COUNT: usize = 5;
}

/// Steps of the stepped-random shape per cycle.
const RANDOM_STEPS: f64 = 256.0;

/// Uniform `[0, 1)` from a step number (a fixed hash, so the sequence is the same for every voice and target).
#[inline]
fn hash01(i: u32) -> f32 {
    let mut x = i.wrapping_mul(0x9E37_79B1) ^ 0x85EB_CA6B;
    x ^= x >> 15;
    x = x.wrapping_mul(0x2C1B_3C6D);
    x ^= x >> 12;
    (x >> 8) as f32 * (1.0 / 16_777_216.0)
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Lfo {
    /// Phase in turns, `[0, 1)`.
    pub phase: f64,
}

impl Lfo {
    pub fn new(phase: f64) -> Self {
        Lfo { phase: phase - libm::floor(phase) }
    }

    /// Advances by `dt` seconds at `hz`.
    pub fn advance(&mut self, dt: f32, hz: f32) {
        self.phase += hz as f64 * dt as f64;
        self.phase -= libm::floor(self.phase);
    }

    /// The unipolar shape value `0..1` at the current phase.
    pub fn shape(&self, kind: usize) -> f32 {
        let ph = self.phase;
        match kind {
            shape::SINE => (sin_turns(ph) + 1.0) * 0.5,
            shape::RAMP_UP => ph as f32,
            shape::RAMP_DOWN => 1.0 - ph as f32,
            shape::SQUARE => {
                if ph < 0.5 {
                    1.0
                } else {
                    0.0
                }
            }
            _ => hash01((ph * RANDOM_STEPS) as u32),
        }
    }
}

/// What a Scale slot sees: `1 − d + d·shape`, `d = (depth·M)²`.
#[inline]
pub fn scale_view(depth: f32, shape: f32) -> f32 {
    let d = depth * depth;
    1.0 - d + d * shape
}

/// What an Offset slot sees: bipolar `d·(2·shape − 1)`.
#[inline]
pub fn offset_view(depth: f32, shape: f32) -> f32 {
    let d = depth * depth;
    d * (2.0 * shape - 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shapes_at_their_phases() {
        crate::tables::init();
        let at = |ph: f64, k| Lfo::new(ph).shape(k);
        assert!((at(0.0, shape::SINE) - 0.5).abs() < 1e-6);
        assert!((at(0.25, shape::SINE) - 1.0).abs() < 1e-6);
        assert!(at(0.75, shape::SINE).abs() < 1e-6);
        assert_eq!((at(0.0, shape::RAMP_UP), at(0.5, shape::RAMP_UP)), (0.0, 0.5));
        assert_eq!((at(0.0, shape::RAMP_DOWN), at(0.25, shape::RAMP_DOWN)), (1.0, 0.75));
        assert_eq!((at(0.0, shape::SQUARE), at(0.49, shape::SQUARE), at(0.5, shape::SQUARE)), (1.0, 1.0, 0.0));
        // stepped random: constant within a step, uniform-looking across steps
        assert_eq!(at(0.0, shape::RANDOM), at(0.001, shape::RANDOM));
        let v: Vec<f32> = (0..256).map(|i| at(i as f64 / 256.0 + 1e-4, shape::RANDOM)).collect();
        assert!(v.iter().all(|x| (0.0..1.0).contains(x)));
        let mean = v.iter().sum::<f32>() / 256.0;
        assert!((mean - 0.5).abs() < 0.08, "{mean}");
        assert!(v.windows(2).any(|w| w[0] != w[1]));
    }

    #[test]
    fn advance_runs_in_hz_and_wraps() {
        let mut l = Lfo::new(0.0);
        for _ in 0..1000 {
            l.advance(0.001, 2.5); // 1 s at 2.5 Hz = 2.5 turns
        }
        assert!((l.phase - 0.5).abs() < 1e-6, "{}", l.phase); // dt is f32
        l.advance(10.0, 0.0);
        assert!((l.phase - 0.5).abs() < 1e-6);
        assert!(Lfo::new(-0.25).phase > 0.74 && Lfo::new(3.5).phase == 0.5);
    }

    #[test]
    fn the_two_views() {
        // depth 1: full range; depth 0.5: d = 0.25
        assert_eq!((scale_view(1.0, 0.0), scale_view(1.0, 1.0)), (0.0, 1.0));
        assert_eq!((offset_view(1.0, 0.0), offset_view(1.0, 1.0), offset_view(1.0, 0.5)), (-1.0, 1.0, 0.0));
        assert!((scale_view(0.5, 0.0) - 0.75).abs() < 1e-7);
        assert!((offset_view(0.5, 1.0) - 0.25).abs() < 1e-7);
        assert_eq!(scale_view(0.0, 0.3), 1.0);
    }
}
