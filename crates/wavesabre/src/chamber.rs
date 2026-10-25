//! Upstream `Chamber`: 8 cross-fed delay lines (4 per channel, lengths 3–31 ms × mode 1/5/10), optional pre-delay,
//! SVF low/high cut on the input.

use crate::helpers::ms_to_len;
use crate::helpers::param_to_frequency;
use crate::svf::{Svf, SvfKind};
use synth_core::device::{Ctx, Device, Io};
use synth_core::dsp::Ring;
use synth_core::mix::{Env, LoadError};
use synth_core::pool::Pool;
use synth_core::rng::Rng;

const LENGTHS: [f32; 8] = [7.0, 21.0, 17.0, 13.0, 3.0, 11.0, 23.0, 31.0];
const MULT: [f32; 3] = [1.0, 5.0, 10.0];
const MAX_PRE_MS: f32 = 500.0;

pub struct Chamber<'a> {
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

impl<'a> Chamber<'a> {
    fn caps(env: &Env) -> ([usize; 8], usize) {
        (LENGTHS.map(|ms| ms_to_len(ms * MULT[2], env.rate)), ms_to_len(MAX_PRE_MS, env.rate))
    }
    pub fn need(_: &[u8], env: &Env) -> Result<usize, LoadError> {
        let (lines, pre) = Self::caps(env);
        Ok(lines.iter().map(|&c| Pool::need::<f32>(c)).sum::<usize>() + 2 * Pool::need::<f32>(pre))
    }
    pub fn new(_: &[u8], env: &Env, pool: &mut Pool<'a>, _: &mut dyn Rng) -> Result<Self, LoadError> {
        let (caps, pre) = Self::caps(env);
        let mut slices = [const { None }; 8];
        for (s, &c) in slices.iter_mut().zip(&caps) {
            *s = Some(Ring::new(pool.slice(c)?));
        }
        Ok(Chamber {
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
}

impl Device for Chamber<'_> {
    fn param_count(&self) -> usize {
        6
    }
    fn set_param(&mut self, i: usize, v: f32) {
        match i {
            0 => self.mode = ((v * 2.0) as i32).clamp(0, 2) as usize, // upstream indexes multipliers unchecked
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
