//! Chorus, Tunefish 3 mode (change add-tf3-import; spec research/tunefish3-functional-spec.md §4.4): one delay tap per
//! channel, `y = x + line[n − d]` where the line stores `x·gain` (no feedback, dry at full level). The delay is
//! `d = 1 + (sin(t·π·rate/sr) + 1)/2 · fi(50·depth²)` ms, `fi(d·sr/1000)` frames, with `t` the song's absolute frame
//! (both channels move together; the output does not depend on how a render is split into calls). Parameters after
//! the mode: rate, depth, gain.

use super::{Effect, Params};
use synth_core::device::{Ctx, Io};
use synth_core::dsp::Ring;
use synth_core::mix::{Env, LoadError};
use synth_core::pool::Pool;

/// Ring length: the original's buffer of sr/10 frames (the longest delay is 51 ms).
fn cap(rate: f64) -> usize {
    (rate / 10.0) as usize + 2
}

/// Round half to even (the original's `fistp`).
fn fi(x: f32) -> i32 {
    libm::rintf(x) as i32
}

pub struct Chorus3<'a> {
    rate_p: f32,
    depth: f32,
    gain: f32,
    line: [Ring<'a>; 2],
}

impl<'a> Chorus3<'a> {
    /// The delay in frames at absolute frame `t`.
    fn delay(&self, t: u64, rate: f64) -> usize {
        let ph = (t as f64 * core::f64::consts::PI * self.rate_p as f64 / rate) as f32;
        let ms = 1.0 + (crate::fm::sinf(ph) + 1.0) / 2.0 * fi(50.0 * self.depth * self.depth) as f32;
        fi(ms * rate as f32 / 1000.0).max(1) as usize
    }
}

impl<'a> Effect<'a> for Chorus3<'a> {
    const PARAMS: usize = 3;
    fn need(env: &Env, _: &Params) -> usize {
        2 * Pool::need::<f32>(cap(env.rate))
    }
    fn new(env: &Env, pool: &mut Pool<'a>) -> Result<Self, LoadError> {
        let a = Ring::new(pool.slice(cap(env.rate))?);
        let b = Ring::new(pool.slice(cap(env.rate))?);
        Ok(Chorus3 { rate_p: 0.0, depth: 0.0, gain: 0.0, line: [a, b] })
    }
    fn set(&mut self, i: usize, v: f32) {
        match i {
            0 => self.rate_p = v,
            1 => self.depth = v,
            2 => self.gain = v,
            _ => {}
        }
    }
    fn reset(&mut self) {
        self.line.iter_mut().for_each(|l| l.clear());
    }
    fn run(&mut self, io: &mut Io, n: usize, ctx: &mut Ctx) {
        let [l, r, ..] = &mut io.ch;
        for i in 0..n {
            let d = self.delay(ctx.frame + i as u64, ctx.rate);
            for (c, v) in [&mut l[i], &mut r[i]].into_iter().enumerate() {
                self.line[c].write(*v * self.gain);
                *v += self.line[c].at(d + 1);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::{noise, run};
    use super::super::{mode_value, UFx, TAG};
    use super::*;
    use crate::kind;

    /// The v3 chunk of the Tunefish 3 chorus: mode 1, then rate, depth, gain.
    fn chunk(rate: f32, depth: f32, gain: f32) -> std::vec::Vec<u8> {
        let mut c: std::vec::Vec<u8> = [mode_value(kind::CHORUS, 1), rate, depth, gain].iter().flat_map(|v| v.to_le_bytes()).collect();
        c.extend(TAG);
        c
    }

    fn render(c: &[u8], l: &[f32], r: &[f32], call: usize) -> (std::vec::Vec<f32>, std::vec::Vec<f32>) {
        let env = Env { rate: 44100.0, tempo: 120, bank: None };
        let mut mem = std::vec![0u8; UFx::need(kind::CHORUS, c, &env).unwrap()];
        let mut pool = Pool::new(&mut mem);
        let mut fx = UFx::new(kind::CHORUS, c, &env, &mut pool).unwrap();
        run(&mut fx, 44100.0, 120, l, r, call)
    }

    #[test]
    fn an_impulse_comes_back_once_at_gain_after_the_delay() {
        // depth 0: no sweep, so the delay is 1 ms = fi(44.1) = 44 frames, whatever the rate
        let mut x = std::vec![0.0f32; 400];
        x[10] = 1.0;
        let (l, r) = render(&chunk(0.7, 0.0, 0.5), &x, &x, 400);
        assert_eq!(l[10], 1.0, "dry at full level");
        assert!((l[10 + 44] - 0.5).abs() < 1e-6, "one tap at gain 0.5, 44 frames later: {}", l[10 + 44]);
        let rest: f32 = l.iter().enumerate().filter(|(k, _)| *k != 10 && *k != 54).map(|(_, v)| v.abs()).sum();
        assert!(rest < 1e-6, "no feedback, no other taps");
        assert_eq!(l, r, "both channels move together");
    }

    #[test]
    fn the_delay_sweeps_between_1_ms_and_1_plus_fi_50_depth_squared_ms() {
        let mut m1 = std::vec![0.0f32; 4];
        let mut m2 = std::vec![0.0f32; 4];
        let c = Chorus3 { rate_p: 1.0, depth: 0.6, gain: 1.0, line: [Ring::new(&mut m1), Ring::new(&mut m2)] };
        // fi(50·0.36) = 18 ms of sweep; sin(t·π/sr) at rate 1 has a period of 2 s
        let (lo, hi) = (0..88_200u64).map(|t| c.delay(t, 44100.0)).fold((usize::MAX, 0), |(a, b), d| (a.min(d), b.max(d)));
        assert_eq!((lo, hi), (44, 838), "1 ms .. 19 ms");
        assert_eq!(c.delay(0, 44100.0), fi(10.0 * 44.1) as usize, "t 0: sin 0, half the sweep: 10 ms");
    }

    #[test]
    fn a_render_split_into_calls_matches_one_call() {
        let (l, r) = noise(3, 9000);
        let c = chunk(0.8, 0.5, 0.7);
        assert_eq!(render(&c, &l, &r, 9000), render(&c, &l, &r, 127));
    }
}
