//! Distortion, power mode (Tunefish): a power-law waveshaper `y = sign(x)·q(|x|)^(1−amount)` with |x| clipped to 1 and
//! quantised to 15 bits. Parameter after the mode: amount.

use super::{Effect, Params};
use synth_core::device::{Ctx, Io};
use synth_core::mix::{Env, LoadError};
use synth_core::pool::Pool;

pub struct Power {
    amount: f32,
}

#[inline]
fn shape(x: f32, exponent: f32) -> f32 {
    let sign = if x > 0.0 {
        1.0
    } else if x < 0.0 {
        -1.0
    } else {
        0.0
    };
    let q = (libm::fabsf(x).min(1.0) * 32767.0) as u32 as f32 / 32768.0;
    sign * crate::fm::pow(q as f64, exponent as f64) as f32
}

impl<'a> Effect<'a> for Power {
    const PARAMS: usize = 1;
    fn need(_: &Env, _: &Params) -> usize {
        0
    }
    fn new(_: &Env, _: &mut Pool<'a>) -> Result<Self, LoadError> {
        Ok(Power { amount: 0.0 })
    }
    fn set(&mut self, i: usize, v: f32) {
        if i == 0 {
            self.amount = v;
        }
    }
    fn reset(&mut self) {}
    fn run(&mut self, io: &mut Io, n: usize, _: &mut Ctx) {
        let e = 1.0 - self.amount;
        for c in 0..2 {
            io.ch[c][..n].iter_mut().for_each(|v| *v = shape(*v, e));
        }
    }
}
