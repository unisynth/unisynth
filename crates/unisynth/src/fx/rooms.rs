//! Reverb, rooms mode (WaveSabre `Chamber`): 8 cross-fed delay lines (4 per channel, lengths 3–31 ms × room size
//! 1/5/10), optional pre-delay, SVF low/high cut on the input. Parameters after the mode: room size, feedback, low
//! cut, high cut, dry/wet, pre-delay.

use super::law::{ms_to_len, param_to_frequency};
use super::svf::{Svf, SvfKind};
use super::{Effect, Params};
use synth_core::device::{Ctx, Io};
use synth_core::dsp::Ring;
use synth_core::mix::{Env, LoadError};
use synth_core::pool::Pool;

const LENGTHS: [f32; 8] = [7.0, 21.0, 17.0, 13.0, 3.0, 11.0, 23.0, 31.0];
const MULT: [f32; 3] = [1.0, 5.0, 10.0];
const MAX_PRE_MS: f32 = 500.0;

pub struct Rooms<'a> {
    mode: usize,
    feedback: f32,
    low_cut: f32,
    high_cut: f32,
    dry_wet: f32,
    pre_delay: f32,
    lines: [Ring<'a>; 8],
    pre: [Ring<'a>; 2],
    lc: [Svf; 2],
    hc: [Svf; 2],
}

fn caps(env: &Env) -> ([usize; 8], usize) {
    (LENGTHS.map(|ms| ms_to_len(ms * MULT[2], env.rate)), ms_to_len(MAX_PRE_MS, env.rate))
}

impl<'a> Effect<'a> for Rooms<'a> {
    const PARAMS: usize = 6;
    fn need(env: &Env, _: &Params) -> usize {
        let (lines, pre) = caps(env);
        lines.iter().map(|&c| Pool::need::<f32>(c)).sum::<usize>() + 2 * Pool::need::<f32>(pre)
    }
    fn new(env: &Env, pool: &mut Pool<'a>) -> Result<Self, LoadError> {
        let (caps, pre) = caps(env);
        let mut slices = [const { None }; 8];
        for (s, &c) in slices.iter_mut().zip(&caps) {
            *s = Some(Ring::new(pool.slice(c)?));
        }
        Ok(Rooms {
            mode: 1,
            feedback: 0.88,
            low_cut: 200.0,
            high_cut: 8000.0,
            dry_wet: 0.27,
            pre_delay: 0.0,
            lines: slices.map(|s| s.unwrap_or_else(|| unreachable!())),
            pre: [Ring::new(pool.slice(pre)?), Ring::new(pool.slice(pre)?)],
            lc: [Svf::new(SvfKind::Highpass); 2],
            hc: [Svf::new(SvfKind::Lowpass); 2],
        })
    }
    fn set(&mut self, i: usize, v: f32) {
        match i {
            0 => self.mode = ((v * 2.0) as i32).clamp(0, 2) as usize, // the original indexes multipliers unchecked
            1 => self.feedback = v * 0.5 + 0.5,
            2 => self.low_cut = param_to_frequency(v),
            3 => self.high_cut = param_to_frequency(v),
            4 => self.dry_wet = v,
            5 => self.pre_delay = v,
            _ => {}
        }
    }
    fn reset(&mut self) {
        self.lines.iter_mut().chain(self.pre.iter_mut()).for_each(Ring::clear);
        self.lc.iter_mut().chain(self.hc.iter_mut()).for_each(Svf::clear);
    }
    fn run(&mut self, io: &mut Io, n: usize, ctx: &mut Ctx) {
        for (l, ms) in self.lines.iter_mut().zip(LENGTHS) {
            l.set_len(ms_to_len(ms * MULT[self.mode], ctx.rate));
        }
        for c in 0..2 {
            self.pre[c].set_len(ms_to_len(self.pre_delay * 500.0, ctx.rate));
            self.lc[c].set_freq(self.low_cut);
            self.hc[c].set_freq(self.high_cut);
        }
        let (fb, dw) = (self.feedback, self.dry_wet);
        for i in 0..n {
            let input = [io.ch[0][i], io.ch[1][i]];
            let mut filtered = [0.0f32; 2];
            for c in 0..2 {
                let x = if self.pre_delay > 0.0 {
                    self.pre[c].write(input[c]);
                    self.pre[c].oldest()
                } else {
                    input[c]
                };
                filtered[c] = self.lc[c].next(self.hc[c].next(x, ctx.rate), ctx.rate);
            }
            let mut out = [0.0f32; 2];
            for j in 0..8 {
                let c = (j < 4) as usize;
                let s = self.lines[7 - j].oldest();
                self.lines[j].write(filtered[c] + s * fb);
                out[c] += input[c] * (1.0 - dw) + s * dw;
            }
            io.ch[0][i] = out[0] / 4.0;
            io.ch[1][i] = out[1] / 4.0;
        }
    }
}
