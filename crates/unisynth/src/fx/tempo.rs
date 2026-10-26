//! Delay, tempo mode (WaveSabre `Echo`): a tempo-synced stereo delay (coarse eighths + fine ms) with cross-feedback
//! and an SVF low and high cut in the feedback path. Parameters after the mode: left coarse, left fine, right coarse,
//! right fine, low cut, high cut, feedback, cross-feedback, dry/wet. The lines are sized at load for the longest
//! setting at the song tempo (16 eighths + 200 ms).

use super::law::{ms_to_len, param_to_frequency};
use super::svf::{Svf, SvfKind};
use super::{Effect, Params};
use synth_core::device::{Ctx, Io};
use synth_core::dsp::Ring;
use synth_core::mix::{Env, LoadError};
use synth_core::pool::Pool;

pub struct Tempo<'a> {
    coarse: [i32; 2],
    fine: [i32; 2],
    low_cut: f32,
    high_cut: f32,
    feedback: f32,
    cross: f32,
    dry_wet: f32,
    buf: [Ring<'a>; 2],
    lc: [Svf; 2],
    hc: [Svf; 2],
}

fn eighth_ms(tempo: i32) -> f64 {
    120.0 / tempo as f64 / 8.0 * 1000.0
}

/// Longest delay: 16 eighths at the song tempo + 200 ms.
fn capacity(env: &Env) -> usize {
    ms_to_len((16.0 * eighth_ms(env.tempo) + 200.0) as f32, env.rate) + 1
}

impl<'a> Effect<'a> for Tempo<'a> {
    const PARAMS: usize = 9;
    fn need(env: &Env, _: &Params) -> usize {
        2 * Pool::need::<f32>(capacity(env))
    }
    fn new(env: &Env, pool: &mut Pool<'a>) -> Result<Self, LoadError> {
        let cap = capacity(env);
        Ok(Tempo {
            coarse: [3, 4],
            fine: [0, 0],
            low_cut: 20.0,
            high_cut: 20000.0 - 20.0,
            feedback: 0.5,
            cross: 0.0,
            dry_wet: 0.5,
            buf: [Ring::new(pool.slice(cap)?), Ring::new(pool.slice(cap)?)],
            lc: [Svf::new(SvfKind::Highpass); 2],
            hc: [Svf::new(SvfKind::Lowpass); 2],
        })
    }
    fn set(&mut self, i: usize, v: f32) {
        match i {
            0 => self.coarse[0] = (v * 16.0) as i32,
            1 => self.fine[0] = (v * 200.0) as i32,
            2 => self.coarse[1] = (v * 16.0) as i32,
            3 => self.fine[1] = (v * 200.0) as i32,
            4 => self.low_cut = param_to_frequency(v),
            5 => self.high_cut = param_to_frequency(v),
            6 => self.feedback = v,
            7 => self.cross = v,
            8 => self.dry_wet = v,
            _ => {}
        }
    }
    fn reset(&mut self) {
        self.buf.iter_mut().for_each(Ring::clear);
        self.lc.iter_mut().chain(self.hc.iter_mut()).for_each(Svf::clear);
    }
    fn run(&mut self, io: &mut Io, n: usize, ctx: &mut Ctx) {
        let scalar = eighth_ms(ctx.tempo);
        for c in 0..2 {
            let ms = (self.coarse[c] as f64 * scalar + self.fine[c] as f64) as f32;
            self.buf[c].set_len(ms_to_len(ms, ctx.rate));
            self.lc[c].set_freq(self.low_cut);
            self.hc[c].set_freq(self.high_cut);
        }
        let (fb, cross, dw) = (self.feedback, self.cross, self.dry_wet);
        for i in 0..n {
            let (li, ri) = (io.ch[0][i], io.ch[1][i]);
            let ld = self.lc[0].next(self.hc[0].next(self.buf[0].oldest(), ctx.rate), ctx.rate);
            let rd = self.lc[1].next(self.hc[1].next(self.buf[1].oldest(), ctx.rate), ctx.rate);
            self.buf[0].write(li + (ld * (1.0 - cross) + rd * cross) * fb);
            self.buf[1].write(ri + (rd * (1.0 - cross) + ld * cross) * fb);
            io.ch[0][i] = li * (1.0 - dw) + ld * dw;
            io.ch[1][i] = ri * (1.0 - dw) + rd * dw;
        }
    }
}
