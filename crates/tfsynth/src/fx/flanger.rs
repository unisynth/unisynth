//! Flanger (spec §4.5): per channel `y = clip(x − wet·buf[n − Δ], ±1)` written back into a 4096-sample ring
//! (inverted feedback), `Δ = trunc(0.1 ms + amp·12 ms·(1 − cos θ)/2)` in samples. The sweep comes from the
//! reference's bouncing counter: it moves by `lfo·0.1·512` per sample, so for nearly any lfo > 0 it flips between 1
//! and 0.01 every sample; each change folds the elapsed sweep into θ_L (rate ∝ f) and θ_R (rate ∝ 1 − f), giving
//! ≈ 0.126 / 0.124 Hz. While f is constant both channels advance 120× slower at the same rate (lfo = 0: both sweep
//! together, period 480 s). Fix: the elapsed-sample counter is folded into the phases every 2²² samples and the
//! phases wrap at 2π, so long runs stay bounded (the reference's counter grows without bound).

use crate::params::{COUNT, FLANGER_AMP, FLANGER_FREQ, FLANGER_LFO, FLANGER_WET};
use synth_core::dsp::Ring;
use synth_core::pool::{OutOfMemory, Pool};

const RING: usize = 4096;
const RATE: f32 = 44100.0;
const TWO_PI: f32 = 2.0 * core::f32::consts::PI;
const FOLD: u32 = 1 << 22;

pub struct Flanger<'a> {
    ring: [Ring<'a>; 2],
    rising: bool,
    counter: f32,
    last: f32,
    /// Samples since the last fold.
    elapsed: u32,
    theta: [f32; 2],
}

impl<'a> Flanger<'a> {
    pub fn need() -> usize {
        2 * Pool::need::<f32>(RING)
    }

    pub fn new(pool: &mut Pool<'a>) -> Result<Self, OutOfMemory> {
        Ok(Flanger {
            ring: [Ring::new(pool.slice(RING)?), Ring::new(pool.slice(RING)?)],
            rising: true,
            counter: 0.0,
            last: 0.0,
            elapsed: 0,
            theta: [0.0; 2],
        })
    }

    pub fn clear(&mut self) {
        self.ring.iter_mut().for_each(|r| r.clear());
        self.rising = true;
        self.counter = 0.0;
        self.last = 0.0;
        self.elapsed = 0;
        self.theta = [0.0; 2];
    }

    /// Divisor of the fold (`sr·4·60/π`, f32) and of the running term (integer `sr·240` / π).
    fn fold_div() -> f32 {
        RATE * 4.0 * 60.0 / core::f32::consts::PI
    }
    fn run_div() -> f32 {
        (44100u32 * 4 * 60) as f32 / core::f32::consts::PI
    }

    fn wrap(t: f32) -> f32 {
        let w = libm::fmodf(t, TWO_PI);
        if w < 0.0 { w + TWO_PI } else { w }
    }

    /// Current delays in samples (left, right) without advancing.
    pub fn deltas(&self, amp: f32) -> [usize; 2] {
        let lo = RATE * 0.1 / 1000.0;
        let hi = RATE * 12.1 / 1000.0;
        let f = if self.counter == 0.0 { 1.0 } else { self.counter };
        let run = self.elapsed as f32 * f / Self::run_div();
        self.theta.map(|t| (lo + ((hi - lo) / 8192.0) * amp * 4096.0 * (1.0 - libm::cosf(t + run))) as usize)
    }

    pub fn run(&mut self, p: &[f32; COUNT], l: &mut [f32], r: &mut [f32]) {
        let (amp, top, lfo, wet) = (p[FLANGER_AMP], p[FLANGER_FREQ], p[FLANGER_LFO], p[FLANGER_WET]);
        let inc = 0.1 * 512.0; // the reference's per-sample step scales with its 512-frame block
        for (lv, rv) in l.iter_mut().zip(r.iter_mut()) {
            if self.rising {
                self.counter += lfo * inc;
                if self.counter > top {
                    self.counter = 1.0;
                    self.rising = false;
                }
            } else {
                self.counter -= lfo * inc;
                if self.counter < 0.0 {
                    self.counter = 0.01;
                    self.rising = true;
                }
            }
            let f = if self.counter == 0.0 { 1.0 } else { self.counter };
            if self.last != f {
                let n = self.elapsed as f64;
                self.theta[0] = Self::wrap(self.theta[0] + (n * f as f64 * 120.0 / Self::fold_div() as f64) as f32);
                self.theta[1] = Self::wrap(self.theta[1] + (n * (1.0 - f) as f64 * 120.0 / Self::fold_div() as f64) as f32);
                self.last = f;
                self.elapsed = 0;
            } else if self.elapsed >= FOLD {
                // fix: fold the running term (same rate as the delay formula below) so the counter stays bounded
                let run = self.elapsed as f32 * f / Self::run_div();
                self.theta = self.theta.map(|t| Self::wrap(t + run));
                self.elapsed = 0;
            }
            let d = self.deltas(amp);
            self.elapsed += 1;
            for (c, v) in [lv, rv].into_iter().enumerate() {
                let y = (*v - wet * self.ring[c].at(d[c])).clamp(-1.0, 1.0);
                *v = y;
                self.ring[c].write(y);
            }
        }
    }

    #[cfg(test)]
    pub fn theta(&self) -> [f32; 2] {
        self.theta
    }
}
