//! Upstream `Cathedral`: Freeverb (8 parallel damped combs into 4 series allpasses per channel, stock 44.1 kHz
//! tunings, right channel +23 on combs and allpasses) on the shared `synth_core` comb/allpass, plus freeze, width,
//! pre-delay and SVF low/high cut on the wet signal.

use crate::helpers::{ms_to_len, param_to_boolean, param_to_frequency};
use crate::svf::{Svf, SvfKind};
use synth_core::device::{Ctx, Device, Io};
use synth_core::dsp::{AllPass, Comb, Ring};
use synth_core::mix::{Env, LoadError};
use synth_core::pool::Pool;
use synth_core::rng::Rng;

const COMBS: [usize; 8] = [1116, 1188, 1277, 1356, 1422, 1491, 1557, 1617];
const ALLPASSES: [usize; 4] = [556, 441, 341, 225];
const SPREAD: usize = 23;
const MAX_PRE_MS: f32 = 500.0;

fn take<T>(o: Option<T>) -> T {
    o.unwrap_or_else(|| unreachable!())
}

pub struct Cathedral<'a> {
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
    combs: [[Comb<'a>; 8]; 2],
    allpasses: [[AllPass<'a>; 4]; 2],
    pre: Ring<'a>,
    lc: [Svf; 2],
    hc: [Svf; 2],
}

impl<'a> Cathedral<'a> {
    pub fn need(_: &[u8], env: &Env) -> Result<usize, LoadError> {
        let lines: usize = COMBS.iter().chain(&ALLPASSES).map(|&l| Pool::need::<f32>(l) + Pool::need::<f32>(l + SPREAD)).sum();
        Ok(lines + Pool::need::<f32>(ms_to_len(MAX_PRE_MS, env.rate)))
    }

    pub fn new(_: &[u8], env: &Env, pool: &mut Pool<'a>, _: &mut dyn Rng) -> Result<Self, LoadError> {
        let mut combs: [[Option<Comb<'a>>; 8]; 2] = Default::default();
        let mut allpasses: [[Option<AllPass<'a>>; 4]; 2] = Default::default();
        for ch in 0..2 {
            for (c, &len) in combs[ch].iter_mut().zip(&COMBS) {
                *c = Some(Comb::new(pool.slice(len + ch * SPREAD)?));
            }
            for (a, &len) in allpasses[ch].iter_mut().zip(&ALLPASSES) {
                let mut ap = AllPass::new(pool.slice(len + ch * SPREAD)?);
                ap.feedback = 0.5; // upstream sets it once from the default room size, never again
                *a = Some(ap);
            }
        }
        let mut d = Cathedral {
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
            combs: combs.map(|c| c.map(take)),
            allpasses: allpasses.map(|a| a.map(take)),
            pre: Ring::new(pool.slice(ms_to_len(MAX_PRE_MS, env.rate))?),
            lc: [Svf::new(SvfKind::Highpass); 2],
            hc: [Svf::new(SvfKind::Lowpass); 2],
        };
        d.update();
        Ok(d)
    }

    /// Upstream `UpdateParams`.
    fn update(&mut self) {
        self.wet = [self.width / 2.0 + 0.5, (1.0 - self.width) / 2.0];
        let (room, damp) = if self.freeze { (1.0, 0.0) } else { (self.room, self.damp) };
        self.gain = if self.freeze { 0.0 } else { 0.015 };
        for c in self.combs.iter_mut().flatten() {
            c.feedback = room;
            c.damp = damp;
        }
    }
}

impl Device for Cathedral<'_> {
    fn param_count(&self) -> usize {
        8
    }
    fn set_param(&mut self, i: usize, v: f32) {
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
        for c in self.combs.iter_mut().flatten() {
            c.clear();
        }
        for a in self.allpasses.iter_mut().flatten() {
            a.clear();
        }
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
            let (mut ol, mut or) = (0.0f32, 0.0f32);
            let [cl, cr] = &mut self.combs;
            for (cl, cr) in cl.iter_mut().zip(cr.iter_mut()) {
                ol += cl.process(input);
                or += cr.process(input);
            }
            let [al, ar] = &mut self.allpasses;
            for (al, ar) in al.iter_mut().zip(ar.iter_mut()) {
                ol = al.process(ol);
                or = ar.process(or);
            }
            ol = self.lc[0].next(self.hc[0].next(ol, ctx.rate), ctx.rate);
            or = self.lc[1].next(self.hc[1].next(or, ctx.rate), ctx.rate);
            ol = ol * w1 + or * w2;
            or = or * w1 + ol * w2; // upstream uses the already-mixed left here
            io.ch[0][s] = li * (1.0 - dw) + ol * dw;
            io.ch[1][s] = ri * (1.0 - dw) + or * dw;
        }
    }
}
