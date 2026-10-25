//! Upstream `StateVariableFilter`: Chamberlin SVF, 2× oversampled (linear midpoint), `f = 1.5·FastSin(π·fc/2/rate)`.
#![allow(dead_code)]

use crate::helpers::fast_sin;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SvfKind {
    Lowpass,
    Highpass,
    Bandpass,
    Notch,
}

impl SvfKind {
    /// Upstream `ParamToStateVariableFilterType`: `(int)(param·3)`.
    pub fn from_param(p: f32) -> Self {
        match (p * 3.0) as i32 {
            1 => SvfKind::Highpass,
            2 => SvfKind::Bandpass,
            3 => SvfKind::Notch,
            _ => SvfKind::Lowpass,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Svf {
    pub kind: SvfKind,
    freq: f32,
    q: f32,
    f: f32,
    rate: f64,
    last: f32,
    low: f32,
    band: f32,
}

impl Svf {
    pub fn new(kind: SvfKind) -> Self {
        Svf { kind, freq: 20.0, q: 1.0, f: 0.0, rate: 0.0, last: 0.0, low: 0.0, band: 0.0 }
    }

    pub fn set_freq(&mut self, freq: f32) {
        if freq != self.freq {
            self.freq = freq;
            self.rate = 0.0; // recompute
        }
    }

    pub fn set_q(&mut self, q: f32) {
        self.q = q;
    }

    pub fn clear(&mut self) {
        self.last = 0.0;
        self.low = 0.0;
        self.band = 0.0;
    }

    #[inline]
    fn step(&mut self, x: f32) -> f32 {
        self.low += self.f * self.band;
        let high = self.q * (x - self.band) - self.low;
        self.band += self.f * high;
        match self.kind {
            SvfKind::Lowpass => self.low,
            SvfKind::Highpass => high,
            SvfKind::Bandpass => self.band,
            SvfKind::Notch => self.low + high,
        }
    }

    #[inline]
    pub fn next(&mut self, x: f32, rate: f64) -> f32 {
        if self.rate != rate {
            self.f = (1.5 * fast_sin(3.141592 * self.freq as f64 / 2.0 / rate)) as f32;
            self.rate = rate;
        }
        let y = (self.step((self.last + x) / 2.0) + self.step(x)) / 2.0;
        self.last = x;
        y
    }
}
