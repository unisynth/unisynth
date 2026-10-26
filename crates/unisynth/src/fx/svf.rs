//! WaveSabre's `StateVariableFilter` for the effect modes: a Chamberlin SVF, 2× oversampled (linear midpoint),
//! `f = 1.5·FastSin(π·fc/2/rate)`. Used as the low/high cut in the feedback path of the tempo, phaser and rooms modes.

use super::law::fast_sin;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SvfKind {
    Lowpass,
    Highpass,
}

#[derive(Clone, Copy)]
pub struct Svf {
    kind: SvfKind,
    freq: f32,
    f: f32,
    rate: f64,
    last: f32,
    low: f32,
    band: f32,
}

impl Svf {
    pub fn new(kind: SvfKind) -> Self {
        Svf { kind, freq: 20.0, f: 0.0, rate: 0.0, last: 0.0, low: 0.0, band: 0.0 }
    }

    pub fn set_freq(&mut self, freq: f32) {
        if freq != self.freq {
            self.freq = freq;
            self.rate = 0.0; // recompute
        }
    }

    pub fn clear(&mut self) {
        self.last = 0.0;
        self.low = 0.0;
        self.band = 0.0;
    }

    #[inline]
    fn step(&mut self, x: f32) -> f32 {
        self.low += self.f * self.band;
        let high = (x - self.band) - self.low;
        self.band += self.f * high;
        match self.kind {
            SvfKind::Lowpass => self.low,
            SvfKind::Highpass => high,
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
