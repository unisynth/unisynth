//! Chorus (Tunefish): three cascaded single-tap lines per channel (`y = x + g·x[n−d]`, g = 0.7p, no feedback),
//! each with its own sine LFO started at a random phase in [0, 1) rad. Once per 512-frame block of the *song's
//! absolute frame count* every line's delay jumps to `clamp(1 + 9·depth·(sin φ + 0.5), 1, 10)` ms (the reference's
//! mis-scaled offset, which leaves a flat spot at 1 ms) and its phase advances by `50·rate²·512/song_rate`
//! radians (the reference's law at 44.1 kHz, here per second at any rate). Phases wrap at 2π. Because the control
//! grid is on absolute frames, the output does not depend on how a render is split into calls. The random phases
//! come from the effect's own fixed-seed generator, drawn again on reset. Parameters after the mode: rate, depth,
//! gain.

use super::law::ms_to_len;
use super::{Effect, Params};
use crate::rng::Xorshift;
use synth_core::device::{Ctx, Io};
use synth_core::dsp::Ring;
use synth_core::mix::{Env, LoadError};
use synth_core::pool::Pool;
use synth_core::rng::Rng;

const LINES: usize = 6;
/// Frames per control block.
const BLOCK: u64 = 512;
const TWO_PI: f32 = 2.0 * core::f32::consts::PI;

/// Ring length: more than 10 ms + 1.
fn cap(rate: f64) -> usize {
    ms_to_len(10.0, rate) + 2
}

pub struct Chorus<'a> {
    rate_p: f32,
    depth: f32,
    gain_p: f32,
    line: [Ring<'a>; LINES],
    phase: [f32; LINES],
    delay: [usize; LINES],
    gain: f32,
    rng: Xorshift,
}

impl<'a> Chorus<'a> {
    /// Block-rate update: delays from the current phases, then the phases advance.
    fn control(&mut self, rate: f64) {
        self.gain = self.gain_p * 0.7;
        let step = (self.rate_p * self.rate_p) / rate as f32 * 512.0 * 50.0;
        for i in 0..LINES {
            let s = crate::fm::sinf(self.phase[i]) + 0.5;
            let ms = (s * self.depth * 9.0 + 1.0).clamp(1.0, 10.0);
            self.delay[i] = ((rate * ms as f64 / 1000.0) as usize).max(1);
            self.phase[i] += step;
            if self.phase[i] >= TWO_PI {
                self.phase[i] -= TWO_PI;
            }
        }
    }

    /// Lines 0, 2, 4 in series on the left, 1, 3, 5 on the right.
    fn process(&mut self, l: &mut [f32], r: &mut [f32]) {
        for i in 0..LINES {
            let buf: &mut [f32] = if i % 2 == 0 { &mut *l } else { &mut *r };
            let (line, d, g) = (&mut self.line[i], self.delay[i], self.gain);
            for v in buf.iter_mut() {
                line.write(*v * g);
                *v += line.at(d + 1);
            }
        }
    }
}

impl<'a> Chorus<'a> {
    #[cfg(test)]
    pub(super) fn phase(&self) -> f32 {
        self.phase[0]
    }
}

impl<'a> Effect<'a> for Chorus<'a> {
    const PARAMS: usize = 3;
    fn need(env: &Env, _: &Params) -> usize {
        LINES * Pool::need::<f32>(cap(env.rate))
    }
    fn new(env: &Env, pool: &mut Pool<'a>) -> Result<Self, LoadError> {
        let mut bufs = [None, None, None, None, None, None];
        for b in bufs.iter_mut() {
            *b = Some(Ring::new(pool.slice(cap(env.rate))?));
        }
        let mut rng = Xorshift::new(0);
        Ok(Chorus {
            rate_p: 0.0,
            depth: 0.0,
            gain_p: 0.0,
            line: bufs.map(|b| b.unwrap()),
            phase: core::array::from_fn(|_| rng.next_f32()),
            delay: [1; LINES],
            gain: 0.0,
            rng,
        })
    }
    fn set(&mut self, i: usize, v: f32) {
        match i {
            0 => self.rate_p = v,
            1 => self.depth = v,
            2 => self.gain_p = v,
            _ => {}
        }
    }
    fn reset(&mut self) {
        self.line.iter_mut().for_each(|l| l.clear());
        self.rng = Xorshift::new(0);
        self.phase = core::array::from_fn(|_| self.rng.next_f32());
        self.delay = [1; LINES];
        self.gain = 0.0;
    }
    fn run(&mut self, io: &mut Io, n: usize, ctx: &mut Ctx) {
        let [l, r, ..] = &mut io.ch;
        let mut i = 0;
        while i < n {
            let at = ((ctx.frame + i as u64) % BLOCK) as usize;
            if at == 0 {
                self.control(ctx.rate);
            }
            let len = (n - i).min(BLOCK as usize - at);
            self.process(&mut l[i..i + len], &mut r[i..i + len]);
            i += len;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::{noise, run, with_fx};
    use super::*;
    use crate::kind;

    /// Phase 0 after `blocks` control updates at `rate`: the random start plus `blocks` steps of `50·p²·512/rate`.
    #[test]
    fn the_phase_advances_by_the_rate_law_per_block_at_any_rate() {
        let rate_p = 0.6f32;
        for rate in [44100.0f64, 48000.0, 96000.0] {
            let n = (rate as usize) * 2;
            let (l, r) = noise(1, n);
            let blocks = n.div_ceil(512);
            let phase = with_fx(kind::CHORUS, &[0.0, rate_p, 0.5, 1.0], rate, 120, |fx| {
                run(fx, rate, 120, &l, &r, 512);
                fx.chorus_phase()
            });
            let start = Xorshift::new(0).next_f32();
            let step = (rate_p * rate_p) / rate as f32 * 512.0 * 50.0;
            let want = (start + blocks as f32 * step) % TWO_PI;
            // f32 accumulation: tolerance of a few ulps per block
            let d = (phase - want).abs().min(TWO_PI - (phase - want).abs());
            assert!(d < 0.02, "rate {rate}: {phase} vs {want}");
        }
    }
}
