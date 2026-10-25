//! Block-rate LFO (spec §3.5): unipolar, multiplicative (`depth·shape + 1 − depth`), 0..≈8 Hz.

use crate::params::lfo_shape;
use crate::tables::{tables, LFO_NOISE_LEN};
use core::f32::consts::PI;

const TWO_PI: f32 = PI * 2.0;
/// Rate law `50·p²·N/sr` radians per block (N = 512, sr = 44 100).
pub fn rate(p: f32) -> f32 {
    (p * p) / 44100.0 * 512.0 * 50.0
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Lfo {
    pub phase: f32,
}

impl Lfo {
    /// One block. `p` = rate, depth, shape (sync is handled at note-on); `depth_mod` = M(depth).
    pub fn step(&mut self, p: &[f32], depth_mod: f32) -> f32 {
        let depth = p[1] * depth_mod;
        let depth = depth * depth;
        let ph = self.phase;
        let shape = match lfo_shape(p[2]) {
            0 => (libm::sinf(ph) + 1.0) / 2.0,
            1 => libm::fmodf(ph, TWO_PI) / TWO_PI,
            2 => 1.0 - libm::fmodf(ph, TWO_PI) / TWO_PI,
            3 => {
                if ph < PI {
                    1.0
                } else {
                    0.0
                }
            }
            4 => tables().lfo_noise[((ph / (PI * 2.0) * LFO_NOISE_LEN as f32) as usize).min(LFO_NOISE_LEN - 1)],
            _ => 1.0,
        };
        self.phase += rate(p[0]);
        if self.phase > TWO_PI {
            self.phase -= TWO_PI;
        }
        shape * depth + (1.0 - depth)
    }
}
