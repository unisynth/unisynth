//! EQ, 3band mode (Tunefish): the musicdsp "3 Band Equaliser": low = four cascaded one-poles at 880 Hz, high = the input
//! delayed 3 samples minus four one-poles at 5 kHz, mid = the delayed input minus both. The corner frequencies
//! are computed for the song's rate (the reference assumes 44.1 kHz). Parameters after the mode: low, mid, high, and
//! an optional pan: 0 (absent from older chunks) leaves the channels, a byte 1..255 pans equal-power from hard left to
//! hard right (`sqrt(1 − q)`, `sqrt(q)`, `q = (byte − 1)/254`) after the bands: a converted XM pans its channel after
//! the effect chain, as the tracker pans the finished sample.
//! A gain change ramps linearly over the block it arrives in (automation steps once per sub-block: a step would
//! click, as a converted XM's volume slide on its post-chain gain did); the first block and a reset take it at once.

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
    pan: f32,
    /// The band gains and the left and right pan gains applied at the end of the last block; `None` before the first
    /// block and after a reset.
    cur: Option<[f32; 5]>,
    ch: [Chan; 2],
}

impl<'a> Effect<'a> for ThreeBand {
    const PARAMS: usize = 4;
    fn need(_: &Env, _: &Params) -> usize {
        0
    }
    fn new(_: &Env, _: &mut Pool<'a>) -> Result<Self, LoadError> {
        Ok(ThreeBand { gain: [0.0; 3], pan: 0.0, cur: None, ch: [Chan::default(); 2] })
    }
    fn set(&mut self, i: usize, v: f32) {
        if i < 3 {
            self.gain[i] = v;
        } else if i == 3 {
            self.pan = v;
        }
    }
    fn reset(&mut self) {
        self.ch = [Chan::default(); 2];
        self.cur = None;
    }
    fn run(&mut self, io: &mut Io, n: usize, ctx: &mut Ctx) {
        let (pl, pr) = if self.pan > 0.0 {
            let q = ((self.pan * 255.0 - 1.0) / 254.0).clamp(0.0, 1.0);
            (crate::fm::sqrtf(1.0 - q), crate::fm::sqrtf(q))
        } else {
            (1.0, 1.0)
        };
        let to = [eq_gain(self.gain[0]), eq_gain(self.gain[1]), eq_gain(self.gain[2]), pl, pr];
        let from = self.cur.unwrap_or(to);
        self.cur = Some(to);
        let step = |k: usize| (to[k] - from[k]) / n.max(1) as f32;
        let (dl, dm, dh) = (step(0), step(1), step(2));
        let rate = ctx.rate as f32;
        let kl = 2.0 * crate::fm::sinf(core::f32::consts::PI * (880.0 / rate));
        let kh = 2.0 * crate::fm::sinf(core::f32::consts::PI * (5000.0 / rate));
        for c in 0..2 {
            let b = &mut self.ch[c];
            let (mut gl, mut gm, mut gh, mut gp, dp) = (from[0], from[1], from[2], from[3 + c], step(3 + c));
            for v in io.ch[c][..n].iter_mut() {
                (gl, gm, gh, gp) = (gl + dl, gm + dm, gh + dh, gp + dp);
                let x = *v;
                let mut lo = x;
                b.low.iter_mut().for_each(|s| lo = s.lowpass(lo, kl));
                let mut hi = x;
                b.high.iter_mut().for_each(|s| hi = s.lowpass(hi, kh));
                let d = b.hist[2];
                let h = d - hi;
                let m = d - (h + lo);
                *v = (lo * gl + m * gm + h * gh) * gp;
                b.hist = [x, b.hist[0], b.hist[1]];
            }
        }
    }
}
