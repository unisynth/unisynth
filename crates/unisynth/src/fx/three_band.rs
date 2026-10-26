//! EQ, 3band mode (Tunefish): the musicdsp "3 Band Equaliser": low = four cascaded one-poles at 880 Hz, high = the input
//! delayed 3 samples minus four one-poles at 5 kHz, mid = the delayed input minus both. The corner frequencies
//! are computed for the song's rate (the reference assumes 44.1 kHz). Parameters after the mode: low, mid, high.

use super::law::eq_gain;
use super::{Effect, Params};
use synth_core::device::{Ctx, Io};
use synth_core::dsp::OnePole;
use synth_core::mix::{Env, LoadError};
use synth_core::pool::Pool;

#[derive(Clone, Copy, Default)]
struct Chan {
    low: [OnePole; 4],
    high: [OnePole; 4],
    hist: [f32; 3],
}

pub struct ThreeBand {
    gain: [f32; 3],
    ch: [Chan; 2],
}

impl<'a> Effect<'a> for ThreeBand {
    const PARAMS: usize = 3;
    fn need(_: &Env, _: &Params) -> usize {
        0
    }
    fn new(_: &Env, _: &mut Pool<'a>) -> Result<Self, LoadError> {
        Ok(ThreeBand { gain: [0.0; 3], ch: [Chan::default(); 2] })
    }
    fn set(&mut self, i: usize, v: f32) {
        if i < 3 {
            self.gain[i] = v;
        }
    }
    fn reset(&mut self) {
        self.ch = [Chan::default(); 2];
    }
    fn run(&mut self, io: &mut Io, n: usize, ctx: &mut Ctx) {
        let (gl, gm, gh) = (eq_gain(self.gain[0]), eq_gain(self.gain[1]), eq_gain(self.gain[2]));
        let rate = ctx.rate as f32;
        let kl = 2.0 * crate::fm::sinf(core::f32::consts::PI * (880.0 / rate));
        let kh = 2.0 * crate::fm::sinf(core::f32::consts::PI * (5000.0 / rate));
        for c in 0..2 {
            let b = &mut self.ch[c];
            for v in io.ch[c][..n].iter_mut() {
                let x = *v;
                let mut lo = x;
                b.low.iter_mut().for_each(|s| lo = s.lowpass(lo, kl));
                let mut hi = x;
                b.high.iter_mut().for_each(|s| hi = s.lowpass(hi, kh));
                let d = b.hist[2];
                let h = d - hi;
                let m = d - (h + lo);
                *v = lo * gl + m * gm + h * gh;
                b.hist = [x, b.hist[0], b.hist[1]];
            }
        }
    }
}
