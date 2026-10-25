//! 3-band EQ (spec §4.7), the musicdsp "3 Band Equaliser" structure: low = four cascaded one-poles at 880 Hz,
//! high = input delayed 3 samples minus four one-poles at 5 kHz, mid = the delayed input minus both; band gains
//! `2p` / `1 + 10(2p − 1)²`. All gains at 1 is not exactly transparent (the low band is not delay-aligned).

use crate::params::{eq_gain, COUNT, EQ_HIGH, EQ_LOW, EQ_MID};
use synth_core::dsp::OnePole;

#[derive(Clone, Copy, Debug, Default)]
struct Band {
    low: [OnePole; 4],
    high: [OnePole; 4],
    hist: [f32; 3],
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Eq {
    ch: [Band; 2],
}

impl Eq {
    pub fn run(&mut self, p: &[f32; COUNT], l: &mut [f32], r: &mut [f32]) {
        let (gl, gm, gh) = (eq_gain(p[EQ_LOW]), eq_gain(p[EQ_MID]), eq_gain(p[EQ_HIGH]));
        let kl = 2.0 * libm::sinf(core::f32::consts::PI * (880.0 / 44100.0));
        let kh = 2.0 * libm::sinf(core::f32::consts::PI * (5000.0 / 44100.0));
        for (c, buf) in [l, r].into_iter().enumerate() {
            let b = &mut self.ch[c];
            for v in buf.iter_mut() {
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
