//! Flanger, comb mode (change add-cauldron-xm-import): mini-cauldron's comb and doubler (Stash, `0x402A7C` and
//! `0x402FF0`) as a real-time effect.
//!
//! - Sweep rate 0: a feed-forward difference comb `y[n] = x[n] − g·x[n − K]` (the original's in-place
//!   `x[i] = x[i + k] − x[i]`, made causal: the converter shifts the notes by K).
//! - Sweep rate s above 0: the doubler, `y[n] = (x[n − K] + x[n − K + s·t])/2`, with `t` the frames since the last
//!   restart and the read offset `s·t` capped at K (the original reads a copy running at 0.99699×, ahead of where it
//!   writes; here K is the latency that makes it causal). A step of the restart parameter sets `t` to 0.
//!
//! Parameters after the mode: delay K (`round(p·16384)` frames at the song rate), amount g, sweep rate (`p·0.01` per
//! frame), restart (any change restarts the sweep).

use super::{Effect, Params};
use synth_core::device::{Ctx, Io};
use synth_core::dsp::Ring;
use synth_core::mix::{Env, LoadError};
use synth_core::pool::Pool;

/// Longest delay in frames (a parameter of 1).
const MAX: usize = 16384;

pub struct Comb<'a> {
    k: usize,
    g: f32,
    s: f32,
    restart: f32,
    /// Frames since the last restart.
    t: u32,
    line: [Ring<'a>; 2],
}

impl<'a> Effect<'a> for Comb<'a> {
    const PARAMS: usize = 4;
    fn need(_: &Env, _: &Params) -> usize {
        2 * Pool::need::<f32>(MAX + 2)
    }
    fn new(_: &Env, pool: &mut Pool<'a>) -> Result<Self, LoadError> {
        Ok(Comb { k: 0, g: 1.0, s: 0.0, restart: 0.0, t: 0, line: [Ring::new(pool.slice(MAX + 2)?), Ring::new(pool.slice(MAX + 2)?)] })
    }
    fn set(&mut self, i: usize, v: f32) {
        match i {
            0 => self.k = libm::roundf(v.clamp(0.0, 1.0) * MAX as f32) as usize,
            1 => self.g = v,
            2 => self.s = v.max(0.0) * 0.01,
            3 => {
                if v != self.restart {
                    self.t = 0;
                }
                self.restart = v;
            }
            _ => {}
        }
    }
    fn reset(&mut self) {
        self.t = 0;
        for l in self.line.iter_mut() {
            l.clear();
        }
    }
    fn run(&mut self, io: &mut Io, n: usize, _: &mut Ctx) {
        let k = self.k;
        for c in 0..2 {
            let line = &mut self.line[c];
            let mut t = self.t;
            for v in io.ch[c][..n].iter_mut() {
                line.write(*v);
                // `at(j + 1)` is x[n − j]
                let late = line.at(k + 1);
                *v = if self.s > 0.0 {
                    let off = (self.s * t as f32).min(k as f32);
                    (late + line.read_frac(k as f32 - off + 1.0)) * 0.5
                } else {
                    *v - self.g * late
                };
                t = t.saturating_add(1);
            }
            if c == 1 {
                self.t = t;
            }
        }
    }
    /// Its line's capacity.
    fn quiet_after(&self) -> Option<u32> {
        Some(self.line.iter().map(|b| b.capacity()).max().unwrap_or(0) as u32)
    }
    /// The sweep counts frames since its restart: the frames its track slept count too.
    fn wake(&mut self, skipped: u32) {
        self.t = self.t.saturating_add(skipped);
    }
}
