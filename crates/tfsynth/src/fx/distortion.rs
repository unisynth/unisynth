//! Distortion (spec §4.1): power-law waveshaper `y = sign(x)·q(|x|)^(1−amount)` with |x| clipped to 1 and quantised
//! to 15 bits (`q = trunc(|x|·32767)/32768`), computed directly instead of from a 32K-entry table.

use crate::params::{COUNT, DIST_AMOUNT};

#[derive(Clone, Copy, Debug, Default)]
pub struct Distortion;

#[inline]
pub fn shape(x: f32, exponent: f32) -> f32 {
    let sign = if x > 0.0 {
        1.0
    } else if x < 0.0 {
        -1.0
    } else {
        0.0
    };
    let q = (libm::fabsf(x).min(1.0) * 32767.0) as u32 as f32 / 32768.0;
    sign * libm::pow(q as f64, exponent as f64) as f32
}

impl Distortion {
    pub fn run(&mut self, p: &[f32; COUNT], l: &mut [f32], r: &mut [f32]) {
        let e = 1.0 - p[DIST_AMOUNT];
        l.iter_mut().chain(r.iter_mut()).for_each(|v| *v = shape(*v, e));
    }
}
