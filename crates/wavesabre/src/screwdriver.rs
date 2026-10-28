//! `Screwdriver` (kind 13): QM's three-segment waveshaper from the qm210/WaveSabre fork (commit a175dd6, branch
//! `fix-flp-null-reference`, MIT like upstream), not in upstream WaveSabre. Used by *Robo Outer Space Epos*
//! (QM / Team210, Revision 2022). Per sample and channel on `|x|`, sign restored:
//! `x ≤ x1`: `(x/x1)^p0·y1`; `x ≤ x2`: `((x−x1)/(x2−x1))^p1·(y2−y1)+y1`; else `((x−x2)/(1−x2))^p2·(1−y2)+y2`;
//! then dry/wet. Params: X1, Y1, X2, Y2, LogPower0..2 (`p = 2^(8v−4)`), DryWet. Quirk kept: X1 is clamped to
//! `0.001..=x2` and X2 to `x1..=0.999` with the *current* other value, so the result depends on the order the
//! parameters are set (a chunk sets X1 while X2 is still its default 0.3).

use crate::helpers::exp2f;
use synth_core::device::{Ctx, Device, Io};
use synth_core::math::{clamp, mix};
use synth_core::mix::{Env, LoadError};
use synth_core::pool::Pool;
use synth_core::rng::Rng;

pub struct Screwdriver {
    x1: f32,
    y1: f32,
    x2: f32,
    y2: f32,
    power: [f32; 3],
    dry_wet: f32,
}

impl Screwdriver {
    pub fn need(_: &[u8], _: &Env) -> Result<usize, LoadError> {
        Ok(0)
    }
    pub fn new(_: &[u8], _: &Env, _: &mut Pool, _: &mut dyn Rng) -> Result<Self, LoadError> {
        Ok(Screwdriver { x1: 0.1, y1: 0.1, x2: 0.3, y2: 0.3, power: [1.0; 3], dry_wet: 1.0 })
    }
}

/// The curve for one sample (shared with nothing: the unified `curve` mode keeps its own copy).
#[inline]
fn shape(s: &Screwdriver, input: f32) -> f32 {
    let x = libm::fabsf(input);
    let sign = if input >= 0.0 { 1.0 } else { -1.0 };
    let r = if x <= s.x1 {
        libm::powf(x / s.x1, s.power[0]) * s.y1
    } else if x <= s.x2 {
        libm::powf((x - s.x1) / (s.x2 - s.x1), s.power[1]) * (s.y2 - s.y1) + s.y1
    } else {
        libm::powf((x - s.x2) / (1.0 - s.x2), s.power[2]) * (1.0 - s.y2) + s.y2
    };
    r * sign
}

impl Device for Screwdriver {
    fn param_count(&self) -> usize {
        8
    }
    fn set_param(&mut self, i: usize, v: f32) {
        match i {
            0 => self.x1 = clamp(v, 0.001, self.x2),
            1 => self.y1 = v,
            2 => self.x2 = clamp(v, self.x1, 0.999),
            3 => self.y2 = v,
            4..=6 => self.power[i - 4] = exp2f(8.0 * v - 4.0),
            7 => self.dry_wet = v,
            _ => {}
        }
    }
    fn reset(&mut self) {}
    fn run(&mut self, io: &mut Io, n: usize, _: &mut Ctx) {
        for c in 0..2 {
            for x in io.ch[c][..n].iter_mut() {
                *x = mix(*x, shape(self, *x), self.dry_wet);
            }
        }
    }
}
