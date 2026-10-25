//! Upstream `Twister`: modulated delay (types 0/1: flanger with ± feedback, fractional read 2..134 samples back) or
//! 6-stage allpass phaser (types 2/3), with vibrato LFO, stereo spread and SVF low/high cut.

use crate::helpers::{ms_to_len, fast_sin, param_to_frequency, param_to_vibrato_freq};
use crate::svf::{Svf, SvfKind};
use synth_core::device::{Ctx, Device, Io};
use synth_core::dsp::Ring;
use synth_core::math::clamp;
use synth_core::mix::{Env, LoadError};
use synth_core::pool::Pool;
use synth_core::rng::Rng;

/// Upstream `AllPassDelay`: first-order allpass, `a1 = (1 − d)/(1 + d)`.
#[derive(Clone, Copy, Default)]
struct AllPass1 {
    a1: f32,
    zm1: f32,
}

impl AllPass1 {
    fn delay(&mut self, d: f32) {
        self.a1 = (1.0 - d) / (1.0 + d);
    }
    fn update(&mut self, x: f32) -> f32 {
        let y = x * -self.a1 + self.zm1;
        self.zm1 = y * self.a1 + x;
        y
    }
}

pub struct Twister<'a> {
    ty: i32,
    amount: f32,
    feedback: f32,
    /// 0 mono, 1 full invert, 2 mod invert.
    spread: i32,
    vib_freq: f64,
    vib_amount: f32,
    vib_phase: f64,
    low_cut: f32,
    high_cut: f32,
    dry_wet: f32,
    last: [f32; 2],
    ap: [[AllPass1; 6]; 2],
    buf: [Ring<'a>; 2],
    lc: [Svf; 2],
    hc: [Svf; 2],
}

impl<'a> Twister<'a> {
    pub fn need(_: &[u8], env: &Env) -> Result<usize, LoadError> {
        Ok(2 * Pool::need::<f32>(ms_to_len(1000.0, env.rate)))
    }
    pub fn new(_: &[u8], env: &Env, pool: &mut Pool<'a>, _: &mut dyn Rng) -> Result<Self, LoadError> {
        let len = ms_to_len(1000.0, env.rate);
        Ok(Twister {
            ty: 0,
            amount: 0.0,
            feedback: 0.0,
            spread: 0,
            vib_freq: param_to_vibrato_freq(0.0),
            vib_amount: 0.0,
            vib_phase: 0.0,
            low_cut: 20.0,
            high_cut: 20000.0 - 20.0,
            dry_wet: 0.5,
            last: [0.0; 2],
            ap: [[AllPass1::default(); 6]; 2],
            buf: [Ring::new(pool.slice(len)?), Ring::new(pool.slice(len)?)],
            lc: [Svf::new(SvfKind::Highpass); 2],
            hc: [Svf::new(SvfKind::Lowpass); 2],
        })
    }

    /// Upstream `ResampleBuffer::ReadPosition(p)`: `at(⌊p⌋)` moved by `frac(p)` toward the next newer sample.
    fn read(buf: &Ring, p: f32) -> f32 {
        let k = p as usize;
        let f = p - libm::floorf(p);
        let s0 = buf.at(k);
        s0 + f * (buf.at(k + buf.len() - 1) - s0)
    }
}

impl Device for Twister<'_> {
    fn param_count(&self) -> usize {
        9
    }
    fn set_param(&mut self, i: usize, v: f32) {
        match i {
            0 => self.ty = (v * 3.0) as i32,
            1 => self.amount = v,
            2 => self.feedback = v,
            3 => self.spread = (v * 2.0) as i32,
            4 => self.vib_freq = param_to_vibrato_freq(v),
            5 => self.vib_amount = v,
            6 => self.low_cut = param_to_frequency(v),
            7 => self.high_cut = param_to_frequency(v),
            8 => self.dry_wet = v,
            _ => {}
        }
    }
    fn reset(&mut self) {
        self.buf.iter_mut().for_each(Ring::clear);
        self.lc.iter_mut().chain(self.hc.iter_mut()).for_each(Svf::clear);
        self.ap = [[AllPass1::default(); 6]; 2];
        self.last = [0.0; 2];
        self.vib_phase = 0.0;
    }
    fn run(&mut self, io: &mut Io, n: usize, ctx: &mut Ctx) {
        let delta = (self.vib_freq / ctx.rate) * 0.25f32 as f64;
        for c in 0..2 {
            self.lc[c].set_freq(self.low_cut);
            self.hc[c].set_freq(self.high_cut);
        }
        let (fb, dw, rate) = (self.feedback, self.dry_wet, ctx.rate);
        for i in 0..n {
            let input = [io.ch[0][i], io.ch[1][i]];
            let m = (fast_sin(self.vib_phase) * self.vib_amount as f64) as f32;
            let left = clamp(self.amount + m, 0.0, 1.0);
            let mut pos = match self.spread {
                1 => [left, 1.0 - clamp(self.amount + m, 0.0, 1.0)],
                2 => [left, clamp(self.amount - m, 0.0, 1.0)],
                _ => [left, left],
            };
            let mut out = [0.0f32; 2];
            match self.ty {
                0 | 1 => {
                    let sign = if self.ty == 0 { 1.0 } else { -1.0 };
                    for c in 0..2 {
                        pos[c] *= 132.0;
                        let r = Self::read(&self.buf[c], pos[c] + 2.0);
                        out[c] = self.hc[c].next(self.lc[c].next(r, rate), rate);
                    }
                    for c in 0..2 {
                        self.buf[c].write(input[c] + sign * (out[c] * fb));
                    }
                }
                2 | 3 => {
                    let sign = if self.ty == 2 { 1.0 } else { -1.0 };
                    for c in 0..2 {
                        self.ap[c].iter_mut().for_each(|a| a.delay(pos[c]));
                    }
                    for c in 0..2 {
                        let mut x = input[c] + sign * (self.last[c] * fb);
                        for a in self.ap[c].iter_mut().rev() {
                            x = a.update(x);
                        }
                        out[c] = self.hc[c].next(self.lc[c].next(x, rate), rate);
                        self.last[c] = out[c];
                    }
                }
                _ => {}
            }
            io.ch[0][i] = (input[0] * (1.0 - dw)) + (out[0] * dw);
            io.ch[1][i] = (input[1] * (1.0 - dw)) + (out[1] * dw);
            self.vib_phase += delta;
        }
    }
}
