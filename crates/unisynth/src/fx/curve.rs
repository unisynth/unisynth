//! Distortion, curve mode: QM's Screwdriver (qm210/WaveSabre fork, MIT, kind 13 there), a three-segment power-curve
//! waveshaper on `|x|` with the sign restored. Parameters after the mode, in the original order and law:
//! X1, Y1, X2, Y2 (break points), LogPower0..2 (exponent `2^(8v−4)` per segment), DryWet. The original clamps X1 to
//! `0.001..=x2` and X2 to `x1..=0.999` against the *current* other value; kept, so a chunk (X1 set while X2 is still
//! 0.3) and automation behave as there.

use super::{Effect, Params};
use synth_core::device::{Ctx, Io};
use synth_core::math::{clamp, mix};
use synth_core::mix::{Env, LoadError};
use synth_core::pool::Pool;

pub struct Curve {
    x1: f32,
    y1: f32,
    x2: f32,
    y2: f32,
    power: [f32; 3],
    dry_wet: f32,
}

impl Curve {
    #[inline]
    fn shape(&self, input: f32) -> f32 {
        let x = libm::fabsf(input);
        let sign = if input >= 0.0 { 1.0 } else { -1.0 };
        let r = if x <= self.x1 {
            libm::powf(x / self.x1, self.power[0]) * self.y1
        } else if x <= self.x2 {
            libm::powf((x - self.x1) / (self.x2 - self.x1), self.power[1]) * (self.y2 - self.y1) + self.y1
        } else {
            libm::powf((x - self.x2) / (1.0 - self.x2), self.power[2]) * (1.0 - self.y2) + self.y2
        };
        r * sign
    }
}

impl<'a> Effect<'a> for Curve {
    const PARAMS: usize = 8;
    fn need(_: &Env, _: &Params) -> usize {
        0
    }
    fn new(_: &Env, _: &mut Pool<'a>) -> Result<Self, LoadError> {
        Ok(Curve { x1: 0.1, y1: 0.1, x2: 0.3, y2: 0.3, power: [1.0; 3], dry_wet: 1.0 })
    }
    fn set(&mut self, i: usize, v: f32) {
        match i {
            0 => self.x1 = clamp(v, 0.001, self.x2),
            1 => self.y1 = v,
            2 => self.x2 = clamp(v, self.x1, 0.999),
            3 => self.y2 = v,
            4..=6 => self.power[i - 4] = libm::powf(2.0, 8.0 * v - 4.0),
            7 => self.dry_wet = v,
            _ => {}
        }
    }
    fn reset(&mut self) {}
    fn run(&mut self, io: &mut Io, n: usize, _: &mut Ctx) {
        for c in 0..2 {
            for i in 0..n {
                let x = io.ch[c][i];
                io.ch[c][i] = mix(x, self.shape(x), self.dry_wet);
            }
        }
    }
}
