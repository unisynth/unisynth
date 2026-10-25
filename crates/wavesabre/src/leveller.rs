//! Upstream `Leveller`: 5-band EQ per channel (high-pass, 3 peaks skipped at 0 dB, low-pass) and master gain.

use crate::biquad::{Kind, WsBiquad};
use crate::helpers::{param_to_db, param_to_frequency, param_to_q};
use synth_core::device::{Ctx, Device, Io};
use synth_core::mix::{Env, LoadError};
use synth_core::pool::Pool;
use synth_core::rng::Rng;

pub struct Leveller {
    /// freq, q, gain per band: low cut, peak 1–3, high cut.
    bands: [[f32; 3]; 5],
    master: f32,
    f: [[WsBiquad; 5]; 2],
}

impl Leveller {
    pub fn need(_: &[u8], _: &Env) -> Result<usize, LoadError> {
        Ok(0)
    }
    pub fn new(_: &[u8], _: &Env, _: &mut Pool, _: &mut dyn Rng) -> Result<Self, LoadError> {
        let ch = [
            WsBiquad::new(Kind::Highpass),
            WsBiquad::new(Kind::Peak),
            WsBiquad::new(Kind::Peak),
            WsBiquad::new(Kind::Peak),
            WsBiquad::new(Kind::Lowpass),
        ];
        Ok(Leveller {
            bands: [[20.0, 1.0, 0.0], [1000.0, 1.0, 0.0], [3000.0, 1.0, 0.0], [7000.0, 1.0, 0.0], [20000.0, 1.0, 0.0]],
            master: 1.0,
            f: [ch; 2],
        })
    }
}

impl Device for Leveller {
    fn param_count(&self) -> usize {
        14
    }
    fn set_param(&mut self, i: usize, v: f32) {
        // order: LowCutFreq, LowCutQ, Peak{1,2,3}{Freq,Gain,Q}, HighCutFreq, HighCutQ, Master
        match i {
            0 => self.bands[0][0] = param_to_frequency(v),
            1 => self.bands[0][1] = param_to_q(v),
            2..=10 => {
                let b = 1 + (i - 2) / 3;
                match (i - 2) % 3 {
                    0 => self.bands[b][0] = param_to_frequency(v),
                    1 => self.bands[b][2] = param_to_db(v, 24.0),
                    _ => self.bands[b][1] = param_to_q(v),
                }
            }
            11 => self.bands[4][0] = param_to_frequency(v),
            12 => self.bands[4][1] = param_to_q(v),
            13 => self.master = v,
            _ => {}
        }
    }
    fn reset(&mut self) {
        self.f.iter_mut().flatten().for_each(WsBiquad::clear);
    }
    fn run(&mut self, io: &mut Io, n: usize, ctx: &mut Ctx) {
        for c in 0..2 {
            for (f, b) in self.f[c].iter_mut().zip(&self.bands) {
                f.set(b[0], b[1], b[2]);
            }
            let [hp, p1, p2, p3, lp] = &mut self.f[c];
            for x in io.ch[c][..n].iter_mut() {
                let mut s = hp.next(*x, ctx.rate);
                if self.bands[1][2] != 0.0 {
                    s = p1.next(s, ctx.rate);
                }
                if self.bands[2][2] != 0.0 {
                    s = p2.next(s, ctx.rate);
                }
                if self.bands[3][2] != 0.0 {
                    s = p3.next(s, ctx.rate);
                }
                *x = lp.next(s, ctx.rate) * self.master;
            }
        }
    }
}
