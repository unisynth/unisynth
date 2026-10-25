//! Voice filters (spec §3.3), coefficients set once per block from (cutoff, resonance) parameters clamped to
//! [0, 1] and [0, 0.85]:
//! - `Ladder`: 4-pole low-pass ladder (two-tap one-pole stages, global negative feedback, cubic output
//!   saturation `y − y³/6`), cutoff `20000p² + 30` Hz;
//! - `Rbj` high-pass / band-pass (constant skirt gain) on `synth-core`'s DF1 biquad, cutoff `10000p² + 30` Hz,
//!   with the reference's log₁₀2 (not ln 2) bandwidth law, so they ring more than the cookbook formula suggests;
//! - `Notch`: zeros on the unit circle, poles at radius q, heuristic passband gain, cutoff `20000p² + 30` Hz, and
//!   one sample of input latency (the reference's), built on the same biquad fed with the previous input.

use core::f32::consts::PI;
use synth_core::dsp::{Biquad, Coefs};

const RATE: f32 = 44100.0;

fn clamp_in(f: f32, q: f32) -> (f32, f32) {
    (f.clamp(0.0, 1.0), q.clamp(0.0, 0.85))
}

#[derive(Clone, Copy, Debug, Default)]
struct LadderState {
    y: [f32; 4],
    old: [f32; 4],
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Ladder {
    k: f32,
    p: f32,
    r: f32,
    ch: [LadderState; 2],
}

impl Ladder {
    pub fn set(&mut self, f: f32, q: f32) {
        let (f, q) = clamp_in(f, q);
        let fc = f * f * 20000.0 + 30.0;
        let f = 2.0 * fc / RATE;
        self.k = 3.6 * f - 1.6 * f * f - 1.0;
        self.p = (self.k + 1.0) * 0.5;
        self.r = q * libm::powf(core::f32::consts::E, (1.0 - self.p) * 1.386249);
    }

    pub fn run(&mut self, l: &mut [f32], r: &mut [f32]) {
        let (k, p, res) = (self.k, self.p, self.r);
        for (c, buf) in [l, r].into_iter().enumerate() {
            let s = &mut self.ch[c];
            let [mut y0, mut y1, mut y2, mut y3] = s.y;
            let [mut o0, mut o1, mut o2, mut o3] = s.old;
            for v in buf.iter_mut() {
                // stage i: y_i = old_i·p + in_i·p − k·y_i, in = x then each stage's new output
                let x = *v - res * y3;
                y0 = (o0 * p + x * p) - k * y0;
                y1 = (o1 * p + y0 * p) - k * y1;
                y2 = (o2 * p + y1 * p) - k * y2;
                y3 = (o3 * p + y2 * p) - k * y3;
                *v = y3 - y3 * y3 * y3 * (1.0 / 6.0);
                (o0, o1, o2, o3) = (x, y0, y1, y2);
            }
            s.y = [y0, y1, y2, y3];
            s.old = [o0, o1, o2, o3];
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RbjKind {
    HighPass,
    BandPass,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Rbj {
    ch: [Biquad; 2],
}

impl Rbj {
    pub fn set(&mut self, kind: RbjKind, f: f32, q: f32) {
        let (f, q) = clamp_in(f, q);
        let f0 = f * f * 10000.0 + 30.0;
        let w0 = 2.0 * PI * f0 / RATE;
        let (sin, cos) = (libm::sinf(w0), libm::cosf(w0));
        let x = libm::log10f(2.0) / 2.0 * (1.0 - q) * w0 / sin;
        let e = libm::expf(x);
        let alpha = sin * (0.5 * (e - 1.0 / e));
        let c = match kind {
            RbjKind::HighPass => Coefs::highpass(cos, alpha),
            RbjKind::BandPass => Coefs::from_ba(sin / 2.0, 0.0, -(sin / 2.0), 1.0 + alpha, -2.0 * cos, 1.0 - alpha),
        };
        self.ch[0].coefs = c;
        self.ch[1].coefs = c;
    }

    pub fn run(&mut self, l: &mut [f32], r: &mut [f32]) {
        for (c, buf) in [l, r].into_iter().enumerate() {
            buf.iter_mut().for_each(|v| *v = self.ch[c].process(*v));
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Notch {
    ch: [Biquad; 2],
    prev: [f32; 2],
}

impl Notch {
    pub fn set(&mut self, f: f32, q: f32) {
        let (f, q) = clamp_in(f, q);
        let f0 = f * f * 20000.0 + 30.0;
        let z = libm::cosf(2.0 * PI * f0 / RATE);
        let b0 = (1.0 - q) * (1.0 - q) / (2.0 * (libm::fabsf(z) + 1.0)) + q;
        let c = Coefs { c1: b0, c2: -2.0 * z * b0, c3: b0, c4: -2.0 * z * q, c5: q * q };
        self.ch[0].coefs = c;
        self.ch[1].coefs = c;
    }

    pub fn run(&mut self, l: &mut [f32], r: &mut [f32]) {
        for (c, buf) in [l, r].into_iter().enumerate() {
            for v in buf.iter_mut() {
                let x = *v;
                *v = self.ch[c].process(self.prev[c]);
                self.prev[c] = x;
            }
        }
    }
}
