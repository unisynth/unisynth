//! Delay, ms mode (Tunefish): per channel a feedback line of `trunc(rate·whole_ms/1000)` samples (the reference
//! hard-codes 44.1 kHz; here the time is in milliseconds at the song rate, up to 1000 ms). The line stores
//! `b = k·(x + b_old)` and the output is `x + b`, so the dry also appears once at gain k (the reference's
//! zero-latency copy) and echoes come at k², k³ …. A time change keeps the contents and clamps the position.
//! Parameters after the mode: left time, right time, decay.

use super::{Effect, Params};
use synth_core::device::{Ctx, Io};
use synth_core::dsp::Ring;
use synth_core::mix::{Env, LoadError};
use synth_core::pool::Pool;

/// 1000 ms at `rate`.
fn max(rate: f64) -> usize {
    (rate as usize).max(1)
}

/// Samples for a delay parameter: whole milliseconds, then `trunc(rate·ms/1000)`, at least 1.
fn length(p: f32, rate: f64) -> usize {
    let ms = (p * 1000.0) as u32 as f64;
    ((rate * ms / 1000.0) as usize).clamp(1, max(rate))
}

pub struct DelayMs<'a> {
    time: [f32; 2],
    decay: f32,
    line: [Ring<'a>; 2],
}

impl<'a> Effect<'a> for DelayMs<'a> {
    const PARAMS: usize = 3;
    fn need(env: &Env, _: &Params) -> usize {
        2 * Pool::need::<f32>(max(env.rate))
    }
    fn new(env: &Env, pool: &mut Pool<'a>) -> Result<Self, LoadError> {
        let n = max(env.rate);
        Ok(DelayMs { time: [0.0; 2], decay: 0.0, line: [Ring::new(pool.slice(n)?), Ring::new(pool.slice(n)?)] })
    }
    fn set(&mut self, i: usize, v: f32) {
        match i {
            0 => self.time[0] = v,
            1 => self.time[1] = v,
            2 => self.decay = v,
            _ => {}
        }
    }
    fn reset(&mut self) {
        for l in self.line.iter_mut() {
            l.set_len_keep(usize::MAX);
            l.clear();
        }
    }
    fn run(&mut self, io: &mut Io, n: usize, ctx: &mut Ctx) {
        let k = self.decay;
        for c in 0..2 {
            let line = &mut self.line[c];
            line.set_len_keep(length(self.time[c], ctx.rate));
            for v in io.ch[c][..n].iter_mut() {
                let b = (*v + line.oldest()) * k;
                line.write(b);
                *v += b;
            }
        }
    }
}
