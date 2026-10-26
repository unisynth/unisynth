//! EQ, parametric mode (WaveSabre `Leveller`): a 5-band EQ per channel (high-pass, three peaks skipped at 0 dB,
//! low-pass) and a master gain. Parameters after the mode: low-cut freq and Q; peaks 1–3 freq, gain, Q; high-cut
//! freq and Q; master.

use super::law::{exp10f, fast_cos, fast_sin, param_to_db, param_to_frequency, param_to_q};
use super::{Effect, Params};
use synth_core::device::{Ctx, Io};
use synth_core::dsp::{Biquad, Coefs};
use synth_core::mix::{Env, LoadError};
use synth_core::pool::Pool;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Lowpass,
    Highpass,
    Peak,
}

/// WaveSabre's biquad: RBJ coefficients recomputed when a setting changes.
#[derive(Clone, Copy)]
struct Band {
    kind: Kind,
    freq: f32,
    q: f32,
    gain: f32,
    dirty: bool,
    f: Biquad,
}

impl Band {
    fn new(kind: Kind) -> Self {
        Band { kind, freq: 1000.0, q: 1.0, gain: 0.0, dirty: true, f: Biquad::default() }
    }

    fn set(&mut self, freq: f32, q: f32, gain: f32) {
        if freq != self.freq || q != self.q || gain != self.gain {
            (self.freq, self.q, self.gain, self.dirty) = (freq, q, gain, true);
        }
    }

    #[inline]
    fn next(&mut self, x: f32, rate: f64) -> f32 {
        if self.dirty {
            let w0 = 2.0 * 3.141592f32 * self.freq / rate as f32;
            let alpha = fast_sin(w0 as f64) as f32 / (2.0 * self.q);
            let cos = fast_cos(w0 as f64) as f32;
            self.f.coefs = match self.kind {
                Kind::Lowpass => Coefs::lowpass(cos, alpha),
                Kind::Highpass => Coefs::highpass(cos, alpha),
                Kind::Peak => Coefs::peak(cos, alpha, exp10f(self.gain / 40.0)),
            };
            self.dirty = false;
        }
        self.f.process(x)
    }

    fn clear(&mut self) {
        let c = self.f.coefs;
        self.f = Biquad::default();
        self.f.coefs = c;
    }
}

pub struct Parametric {
    /// freq, q, gain per band: low cut, peak 1–3, high cut.
    bands: [[f32; 3]; 5],
    master: f32,
    f: [[Band; 5]; 2],
}

impl<'a> Effect<'a> for Parametric {
    const PARAMS: usize = 14;
    fn need(_: &Env, _: &Params) -> usize {
        0
    }
    fn new(_: &Env, _: &mut Pool<'a>) -> Result<Self, LoadError> {
        let ch = [Band::new(Kind::Highpass), Band::new(Kind::Peak), Band::new(Kind::Peak), Band::new(Kind::Peak), Band::new(Kind::Lowpass)];
        Ok(Parametric {
            bands: [[20.0, 1.0, 0.0], [1000.0, 1.0, 0.0], [3000.0, 1.0, 0.0], [7000.0, 1.0, 0.0], [20000.0, 1.0, 0.0]],
            master: 1.0,
            f: [ch; 2],
        })
    }
    fn set(&mut self, i: usize, v: f32) {
        match i {
            0 => self.bands[0][0] = param_to_frequency(v),
            1 => self.bands[0][1] = param_to_q(v),
            2..=10 => {
                let b = 1 + (i - 2) / 3;
                match (i - 2) % 3 {
                    0 => self.bands[b][0] = param_to_frequency(v),
                    1 => self.bands[b][2] = param_to_db(v, 24.0),
                    _ => self.bands[b][1] = param_to_q(v),
                }
            }
            11 => self.bands[4][0] = param_to_frequency(v),
            12 => self.bands[4][1] = param_to_q(v),
            13 => self.master = v,
            _ => {}
        }
    }
    fn reset(&mut self) {
        self.f.iter_mut().flatten().for_each(Band::clear);
    }
    fn run(&mut self, io: &mut Io, n: usize, ctx: &mut Ctx) {
        for c in 0..2 {
            for (f, b) in self.f[c].iter_mut().zip(&self.bands) {
                f.set(b[0], b[1], b[2]);
            }
            let [hp, p1, p2, p3, lp] = &mut self.f[c];
            for x in io.ch[c][..n].iter_mut() {
                let mut s = hp.next(*x, ctx.rate);
                if self.bands[1][2] != 0.0 {
                    s = p1.next(s, ctx.rate);
                }
                if self.bands[2][2] != 0.0 {
                    s = p2.next(s, ctx.rate);
                }
                if self.bands[3][2] != 0.0 {
                    s = p3.next(s, ctx.rate);
                }
                *x = lp.next(s, ctx.rate) * self.master;
            }
        }
    }
}
