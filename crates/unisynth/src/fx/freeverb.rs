//! Reverb, freeverb mode (WaveSabre `Cathedral`): Freeverb plus freeze, width, pre-delay and an SVF low/high cut
//! on the wet signal. Parameters after the mode: freeze, room, damp, width, low cut, high cut, dry/wet, pre-delay.

use super::law::{ms_to_len, param_to_boolean, param_to_frequency};
use super::reverb::Combs;
use super::svf::{Svf, SvfKind};
use super::{Effect, Params};
use synth_core::device::{Ctx, Io};
use synth_core::dsp::Ring;
use synth_core::mix::{Env, LoadError};
use synth_core::pool::Pool;

const MAX_PRE_MS: f32 = 500.0;

pub struct FreeverbReverb<'a> {
    freeze: bool,
    room: f32,
    damp: f32,
    width: f32,
    low_cut: f32,
    high_cut: f32,
    dry_wet: f32,
    pre_delay: f32,
    gain: f32,
    wet: [f32; 2],
    core: Combs<'a>,
    pre: Ring<'a>,
    lc: [Svf; 2],
    hc: [Svf; 2],
}

impl<'a> FreeverbReverb<'a> {
    fn update(&mut self) {
        self.wet = [self.width / 2.0 + 0.5, (1.0 - self.width) / 2.0];
        let (room, damp) = if self.freeze { (1.0, 0.0) } else { (self.room, self.damp) };
        self.gain = if self.freeze { 0.0 } else { 0.015 };
        self.core.set(room, damp);
    }
}

impl<'a> Effect<'a> for FreeverbReverb<'a> {
    const PARAMS: usize = 8;
    fn need(env: &Env, _: &Params) -> usize {
        Combs::need(env.rate, true) + Pool::need::<f32>(ms_to_len(MAX_PRE_MS, env.rate))
    }
    fn new(env: &Env, pool: &mut Pool<'a>) -> Result<Self, LoadError> {
        let mut d = FreeverbReverb {
            freeze: false,
            room: 0.5,
            damp: 0.0,
            width: 1.0,
            low_cut: 20.0,
            high_cut: 20000.0 - 20.0,
            dry_wet: 0.25,
            pre_delay: 0.0,
            gain: 0.0,
            wet: [0.0; 2],
            core: Combs::new(env.rate, true, pool)?,
            pre: Ring::new(pool.slice(ms_to_len(MAX_PRE_MS, env.rate))?),
            lc: [Svf::new(SvfKind::Highpass); 2],
            hc: [Svf::new(SvfKind::Lowpass); 2],
        };
        d.update();
        Ok(d)
    }
    fn set(&mut self, i: usize, v: f32) {
        match i {
            0 => self.freeze = param_to_boolean(v),
            1 => self.room = v,
            2 => self.damp = v,
            3 => self.width = v,
            4 => self.low_cut = param_to_frequency(v),
            5 => self.high_cut = param_to_frequency(v),
            6 => self.dry_wet = v,
            7 => self.pre_delay = v,
            _ => {}
        }
        if i < 4 {
            self.update();
        }
    }
    fn reset(&mut self) {
        self.core.clear();
        self.pre.clear();
        self.lc.iter_mut().chain(self.hc.iter_mut()).for_each(Svf::clear);
    }
    fn run(&mut self, io: &mut Io, n: usize, ctx: &mut Ctx) {
        for c in 0..2 {
            self.lc[c].set_freq(self.low_cut);
            self.hc[c].set_freq(self.high_cut);
        }
        self.pre.set_len(ms_to_len(self.pre_delay * 500.0, ctx.rate));
        let dw = self.dry_wet;
        let [w1, w2] = self.wet;
        for s in 0..n {
            let (li, ri) = (io.ch[0][s], io.ch[1][s]);
            let mut input = (li + ri) * self.gain;
            if self.pre_delay > 0.0 {
                self.pre.write(input);
                input = self.pre.oldest();
            }
            let [mut ol, mut or] = self.core.process(input);
            ol = self.lc[0].next(self.hc[0].next(ol, ctx.rate), ctx.rate);
            or = self.lc[1].next(self.hc[1].next(or, ctx.rate), ctx.rate);
            ol = ol * w1 + or * w2;
            or = or * w1 + ol * w2; // the original uses the already-mixed left here
            io.ch[0][s] = li * (1.0 - dw) + ol * dw;
            io.ch[1][s] = ri * (1.0 - dw) + or * dw;
        }
    }
}
