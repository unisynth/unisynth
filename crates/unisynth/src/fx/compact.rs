//! Reverb, compact mode (Tunefish): the Freeverb core with input `(L + R)·0.015`; feedback `0.7 + 0.28·room`, damping
//! `0.4·damp`, allpass feedback 0.5; wet `3·wet` split by width, dry `2·(1 − wet)`. Both channels use the same comb
//! lengths, only the right allpasses are 23 samples longer. Parameters after the mode: room, damp, wet, width.

use super::reverb::Combs;
use super::{Effect, Params};
use synth_core::device::{Ctx, Io};
use synth_core::mix::{Env, LoadError};
use synth_core::pool::Pool;

pub struct Compact<'a> {
    room: f32,
    damp: f32,
    wet: f32,
    width: f32,
    core: Combs<'a>,
}

impl<'a> Effect<'a> for Compact<'a> {
    const PARAMS: usize = 4;
    fn need(env: &Env, _: &Params) -> usize {
        Combs::need(env.rate, false)
    }
    fn new(env: &Env, pool: &mut Pool<'a>) -> Result<Self, LoadError> {
        Ok(Compact { room: 0.0, damp: 0.0, wet: 0.0, width: 0.0, core: Combs::new(env.rate, false, pool)? })
    }
    fn set(&mut self, i: usize, v: f32) {
        match i {
            0 => self.room = v,
            1 => self.damp = v,
            2 => self.wet = v,
            3 => self.width = v,
            _ => {}
        }
    }
    fn reset(&mut self) {
        self.core.clear();
    }
    fn run(&mut self, io: &mut Io, n: usize, _: &mut Ctx) {
        let feedback = self.room * 0.28 + 0.7;
        let damp = self.damp * 0.4;
        let wet = self.wet * 3.0;
        let dry = (1.0 - self.wet) * 2.0;
        let width = self.width;
        let (wet1, wet2) = (wet * (width / 2.0 + 0.5), wet * ((1.0 - width) / 2.0));
        self.core.set(feedback, damp);
        let [l, r, ..] = &mut io.ch;
        for (lv, rv) in l[..n].iter_mut().zip(r[..n].iter_mut()) {
            let input = (*lv + *rv) * 0.015;
            let w = self.core.process(input);
            *lv = w[0] * wet1 + w[1] * wet2 + *lv * dry;
            *rv = w[1] * wet1 + w[0] * wet2 + *rv * dry;
        }
    }
}
