//! Crusher (WaveSabre `Crusher`): bit-depth (vertical) and sample-and-hold rate (horizontal) reduction. Parameters
//! after the mode: vertical, horizontal, dry/wet.

use super::law::{exp2f, pow2f};
use super::{Effect, Params};
use synth_core::device::{Ctx, Io};
use synth_core::math::mix;
use synth_core::mix::{Env, LoadError};
use synth_core::pool::Pool;

pub struct Crusher {
    vertical: f32,
    horizontal: f32,
    dry_wet: f32,
    phase: [f32; 2],
    hold: [f32; 2],
}

impl<'a> Effect<'a> for Crusher {
    const PARAMS: usize = 3;
    fn need(_: &Env, _: &Params) -> usize {
        0
    }
    fn new(_: &Env, _: &mut Pool<'a>) -> Result<Self, LoadError> {
        Ok(Crusher { vertical: 0.0, horizontal: 0.0, dry_wet: 1.0, phase: [0.0; 2], hold: [0.0; 2] })
    }
    fn set(&mut self, i: usize, v: f32) {
        match i {
            0 => self.vertical = v,
            1 => self.horizontal = v,
            2 => self.dry_wet = v,
            _ => {}
        }
    }
    fn reset(&mut self) {
        self.phase = [0.0; 2];
        self.hold = [0.0; 2];
    }
    fn run(&mut self, io: &mut Io, n: usize, _: &mut Ctx) {
        let step = 1.0 / exp2f((1.0 - self.vertical) * 15.0 + 1.0);
        let freq = pow2f(1.0 - self.horizontal);
        for c in 0..2 {
            for x in io.ch[c][..n].iter_mut() {
                let input = *x;
                self.phase[c] += freq;
                if self.phase[c] >= 1.0 {
                    self.phase[c] -= 1.0;
                    self.hold[c] = libm::floorf(input / step + 0.5) * step;
                }
                *x = mix(input, self.hold[c], self.dry_wet);
            }
        }
    }
}
