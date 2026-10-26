//! Flanger, ring mode (Tunefish): per channel `y = clip(x − wet·buf[n − Δ], ±1)` written back into a 4096-sample ring
//! (inverted feedback), `Δ = trunc(0.1 ms + amp·12 ms·(1 − cos θ)/2)` in samples. The sweep comes from the
//! reference's bouncing counter: it moves by `lfo·0.1·512` per sample, so for nearly any lfo > 0 it flips between 1
//! and 0.01 every sample; each change folds the elapsed sweep into θ_L (rate ∝ f) and θ_R (rate ∝ 1 − f), giving
//! ≈ 0.126 / 0.124 Hz. While f is constant both channels advance 120× slower at the same rate (lfo = 0: both sweep
//! together, period 480 s). The elapsed-sample counter is folded into the phases every 2²² samples and the phases
//! wrap at 2π, so long runs stay bounded. The reference's constants assume 44.1 kHz; here the phase advance per
//! sample and the delay range (0.1–12.1 ms) are in seconds at the song rate. Parameters after the mode: lfo, freq
//! (the counter's top), amp, wet.

use super::{Effect, Params};
use synth_core::device::{Ctx, Io};
use synth_core::dsp::Ring;
use synth_core::mix::{Env, LoadError};
use synth_core::pool::Pool;

const RING: usize = 4096;
const TWO_PI: f32 = 2.0 * core::f32::consts::PI;
const FOLD: u32 = 1 << 22;

pub struct RingFlanger<'a> {
    lfo: f32,
    top: f32,
    amp: f32,
    wet: f32,
    ring: [Ring<'a>; 2],
    rising: bool,
    counter: f32,
    last: f32,
    /// Samples since the last fold.
    elapsed: u32,
    theta: [f32; 2],
}

/// The song rate as the reference's `sr` and its integer form.
struct Rate {
    hz: f32,
    int: u32,
}

impl Rate {
    fn of(rate: f64) -> Self {
        Rate { hz: rate as f32, int: rate as u32 }
    }
    /// Divisor of the fold (`sr·4·60/π`, f32) and of the running term (integer `sr·240` / π).
    fn fold_div(&self) -> f32 {
        self.hz * 4.0 * 60.0 / core::f32::consts::PI
    }
    fn run_div(&self) -> f32 {
        (self.int * 4 * 60) as f32 / core::f32::consts::PI
    }
}

fn wrap(t: f32) -> f32 {
    let w = libm::fmodf(t, TWO_PI);
    if w < 0.0 { w + TWO_PI } else { w }
}

impl<'a> RingFlanger<'a> {
    /// Current delays in samples (left, right) without advancing.
    fn deltas(&self, rate: &Rate) -> [usize; 2] {
        let lo = rate.hz * 0.1 / 1000.0;
        let hi = rate.hz * 12.1 / 1000.0;
        let f = if self.counter == 0.0 { 1.0 } else { self.counter };
        let run = self.elapsed as f32 * f / rate.run_div();
        self.theta.map(|t| (lo + ((hi - lo) / 8192.0) * self.amp * 4096.0 * (1.0 - crate::fm::cosf(t + run))) as usize)
    }
}

impl<'a> RingFlanger<'a> {
    #[cfg(test)]
    pub(super) fn theta(&self) -> [f32; 2] {
        self.theta
    }
}

impl<'a> Effect<'a> for RingFlanger<'a> {
    const PARAMS: usize = 4;
    fn need(_: &Env, _: &Params) -> usize {
        2 * Pool::need::<f32>(RING)
    }
    fn new(_: &Env, pool: &mut Pool<'a>) -> Result<Self, LoadError> {
        Ok(RingFlanger {
            lfo: 0.0,
            top: 0.0,
            amp: 0.0,
            wet: 0.0,
            ring: [Ring::new(pool.slice(RING)?), Ring::new(pool.slice(RING)?)],
            rising: true,
            counter: 0.0,
            last: 0.0,
            elapsed: 0,
            theta: [0.0; 2],
        })
    }
    fn set(&mut self, i: usize, v: f32) {
        match i {
            0 => self.lfo = v,
            1 => self.top = v,
            2 => self.amp = v,
            3 => self.wet = v,
            _ => {}
        }
    }
    fn reset(&mut self) {
        self.ring.iter_mut().for_each(|r| r.clear());
        self.rising = true;
        self.counter = 0.0;
        self.last = 0.0;
        self.elapsed = 0;
        self.theta = [0.0; 2];
    }
    fn run(&mut self, io: &mut Io, n: usize, ctx: &mut Ctx) {
        let rate = Rate::of(ctx.rate);
        let (top, lfo, wet) = (self.top, self.lfo, self.wet);
        let inc = 0.1 * 512.0; // the reference's per-sample step scales with its 512-frame block
        let [l, r, ..] = &mut io.ch;
        for (lv, rv) in l[..n].iter_mut().zip(r[..n].iter_mut()) {
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
                let e = self.elapsed as f64;
                self.theta[0] = wrap(self.theta[0] + (e * f as f64 * 120.0 / rate.fold_div() as f64) as f32);
                self.theta[1] = wrap(self.theta[1] + (e * (1.0 - f) as f64 * 120.0 / rate.fold_div() as f64) as f32);
                self.last = f;
                self.elapsed = 0;
            } else if self.elapsed >= FOLD {
                let run = self.elapsed as f32 * f / rate.run_div();
                self.theta = self.theta.map(|t| wrap(t + run));
                self.elapsed = 0;
            }
            let d = self.deltas(&rate);
            self.elapsed += 1;
            for (c, v) in [lv, rv].into_iter().enumerate() {
                let y = (*v - wet * self.ring[c].at(d[c])).clamp(-1.0, 1.0);
                *v = y;
                self.ring[c].write(y);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// At θ = π with amp 1 the delay is the top of the range, 12.1 ms, at any song rate.
    #[test]
    fn the_longest_delay_is_12_1_ms_at_any_rate() {
        for (rate, want) in [(44100.0, 533), (48000.0, 580), (96000.0, 1161)] {
            let env = Env { rate, tempo: 120, bank: None };
            let mut mem = std::vec![0u8; 2 * (4 * RING + 16) + 64];
            let mut pool = Pool::new(&mut mem);
            let mut f = <RingFlanger as Effect>::new(&env, &mut pool).unwrap();
            f.amp = 1.0;
            f.theta = [core::f32::consts::PI; 2];
            assert_eq!(f.deltas(&Rate::of(rate)), [want; 2], "rate {rate}");
        }
    }
}
