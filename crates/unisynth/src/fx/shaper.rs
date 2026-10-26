//! Distortion, shaper mode (WaveSabre `Scissor`): a waveshaper (clipper with foldover, sine, parabola) with
//! 1×/2×/4× linear-interpolated oversampling. Parameters after the mode, in the original's order: drive, threshold,
//! foldover, dry/wet, shaper, oversampling.

use super::law::pow2f;
use super::{Effect, Params};
use synth_core::device::{Ctx, Io};
use synth_core::math::mix;
use synth_core::mix::{Env, LoadError};
use synth_core::pool::Pool;

pub struct Shaper {
    drive: f32,
    threshold: f32,
    foldover: f32,
    dry_wet: f32,
    /// 0 clipper, 1 sine, 2 parabola.
    shaper: u32,
    /// 0 ×1, 1 ×2, 2 ×4.
    oversampling: u32,
    last: [f32; 2],
}

impl Shaper {
    #[inline]
    fn distort(&self, v: f32, drive: f32) -> f32 {
        let mut v = v / self.threshold;
        v *= drive;
        match self.shaper {
            0 => {
                if self.foldover > 0.0 {
                    if v < -1.0 {
                        v = -1.0 + (-1.0 - v) * self.foldover;
                    } else if v > 1.0 {
                        v = 1.0 + (1.0 - v) * self.foldover;
                    }
                }
            }
            1 => v = v * 3.141592 / 2.0,
            2 => v = v * v * v,
            _ => {}
        }
        v.clamp(-1.0, 1.0) * self.threshold
    }
}

impl<'a> Effect<'a> for Shaper {
    const PARAMS: usize = 6;
    fn need(_: &Env, _: &Params) -> usize {
        0
    }
    fn new(_: &Env, _: &mut Pool<'a>) -> Result<Self, LoadError> {
        Ok(Shaper { drive: 0.2, threshold: 0.8, foldover: 0.0, dry_wet: 1.0, shaper: 0, oversampling: 0, last: [0.0; 2] })
    }
    fn set(&mut self, i: usize, v: f32) {
        match i {
            0 => self.drive = v,
            1 => self.threshold = v,
            2 => self.foldover = v,
            3 => self.dry_wet = v,
            4 => self.shaper = (v * 2.0) as u32,
            5 => self.oversampling = (v * 2.0) as u32,
            _ => {}
        }
    }
    fn reset(&mut self) {
        self.last = [0.0; 2];
    }
    fn run(&mut self, io: &mut Io, n: usize, _: &mut Ctx) {
        let drive = if self.drive < 0.2 { 1.0 - (0.2 - self.drive) } else { 1.0 + pow2f((self.drive - 0.2) * 5.0) * 5.0 };
        for c in 0..2 {
            for x in io.ch[c][..n].iter_mut() {
                let input = *x;
                let last = self.last[c];
                let mut v = self.distort(input, drive);
                match self.oversampling {
                    1 => {
                        let mid = (last + input) * 0.5;
                        v = (self.distort(mid, drive) + v) * 0.5;
                    }
                    2 => {
                        let mid = (last + input) * 0.5;
                        let q1 = (last + mid) * 0.5;
                        let q2 = (mid + input) * 0.5;
                        v = (self.distort(q1, drive) + self.distort(mid, drive) + self.distort(q2, drive) + v) * 0.25;
                    }
                    _ => {}
                }
                *x = mix(input, v, self.dry_wet);
                self.last[c] = input;
            }
        }
    }
}
