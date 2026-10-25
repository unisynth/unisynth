//! Upstream `BiquadFilter`: RBJ coefficients from FastSin/FastCos at an f32 `w0`, recomputed when a setting changes,
//! on top of the shared `synth_core` DF1 core and designers.
#![allow(dead_code)]

use crate::helpers::{exp10f, fast_cos, fast_sin};
use synth_core::dsp::{Biquad, Coefs};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Lowpass,
    Highpass,
    Peak,
}

#[derive(Clone, Copy, Debug)]
pub struct WsBiquad {
    kind: Kind,
    freq: f32,
    q: f32,
    gain: f32,
    dirty: bool,
    f: Biquad,
}

impl WsBiquad {
    pub fn new(kind: Kind) -> Self {
        WsBiquad { kind, freq: 1000.0, q: 1.0, gain: 0.0, dirty: true, f: Biquad::default() }
    }

    pub fn set(&mut self, freq: f32, q: f32, gain: f32) {
        if freq != self.freq || q != self.q || gain != self.gain {
            self.freq = freq;
            self.q = q;
            self.gain = gain;
            self.dirty = true;
        }
    }

    #[inline]
    pub fn next(&mut self, x: f32, rate: f64) -> f32 {
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

    /// Clears the filter history (coefficients stay).
    pub fn clear(&mut self) {
        let c = self.f.coefs;
        self.f = Biquad::default();
        self.f.coefs = c;
    }
}
