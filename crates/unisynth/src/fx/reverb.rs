//! The Combs core shared by the reverb modes (Jezar's public-domain topology): 8 parallel damped combs into 4
//! series allpasses per channel. The stock tunings are in samples at 44.1 kHz; here they are scaled to the song's
//! rate (unchanged at 44.1 kHz), so a room keeps its size in seconds. The freeverb mode also lengthens the right channel's
//! combs by the stereo spread; the compact mode lengthens only the right allpasses.

use synth_core::dsp::{AllPass, Comb};
use synth_core::mix::LoadError;
use synth_core::pool::Pool;

const COMBS: [usize; 8] = [1116, 1188, 1277, 1356, 1422, 1491, 1557, 1617];
const ALLPASSES: [usize; 4] = [556, 441, 341, 225];
const SPREAD: usize = 23;

/// A stock length at `rate`.
fn scaled(n: usize, rate: f64) -> usize {
    ((n as f64 * rate / 44100.0) as usize).max(1)
}

fn lens(rate: f64, comb_spread: bool) -> ([[usize; 8]; 2], [[usize; 4]; 2]) {
    let sp = scaled(SPREAD, rate);
    let comb = |ch: usize| COMBS.map(|n| scaled(n, rate) + if comb_spread { ch * sp } else { 0 });
    let all = |ch: usize| ALLPASSES.map(|n| scaled(n, rate) + ch * sp);
    ([comb(0), comb(1)], [all(0), all(1)])
}

/// Output (and input) magnitude below which the reverb counts as silent: −140 dBFS, far under one 16-bit step.
const QUIET: f32 = 1e-7;
/// Silent frames after which the reverb sleeps. Every comb stores one loop of at most 1640 frames at 44.1 kHz (3 600 at
/// 96 kHz) and its output is that loop read back, so a silent output over more than the longest loop means the whole
/// state is as quiet as the output: the lines are then cleared and the reverb costs nothing until the input returns.
pub(super) const SLEEP: u32 = 8192;

pub struct Combs<'a> {
    comb: [[Comb<'a>; 8]; 2],
    allpass: [[AllPass<'a>; 4]; 2],
    /// Consecutive frames with a silent input and a silent output.
    quiet: u32,
}

impl<'a> Combs<'a> {
    /// Pool bytes for the 32 + ... lines at `rate`.
    pub fn need(rate: f64, comb_spread: bool) -> usize {
        let (c, a) = lens(rate, comb_spread);
        c.iter().flatten().chain(a.iter().flatten()).map(|&n| Pool::need::<f32>(n)).sum()
    }

    pub fn new(rate: f64, comb_spread: bool, pool: &mut Pool<'a>) -> Result<Self, LoadError> {
        let (c, a) = lens(rate, comb_spread);
        let mut comb: [[Option<Comb<'a>>; 8]; 2] = Default::default();
        let mut allpass: [[Option<AllPass<'a>>; 4]; 2] = Default::default();
        for ch in 0..2 {
            for (slot, &n) in comb[ch].iter_mut().zip(&c[ch]) {
                *slot = Some(Comb::new(pool.slice(n)?));
            }
            for (slot, &n) in allpass[ch].iter_mut().zip(&a[ch]) {
                let mut ap = AllPass::new(pool.slice(n)?);
                ap.feedback = 0.5; // set once, never changed (both original reverbs)
                *slot = Some(ap);
            }
        }
        Ok(Combs {
            comb: comb.map(|c| c.map(|x| x.unwrap_or_else(|| unreachable!()))),
            allpass: allpass.map(|a| a.map(|x| x.unwrap_or_else(|| unreachable!()))),
            quiet: 0,
        })
    }

    pub fn clear(&mut self) {
        self.comb.iter_mut().flatten().for_each(|c| c.clear());
        self.allpass.iter_mut().flatten().for_each(|a| a.clear());
        self.quiet = 0;
    }

    pub fn set(&mut self, feedback: f32, damp: f32) {
        for c in self.comb.iter_mut().flatten() {
            c.feedback = feedback;
            c.damp = damp;
        }
    }

    /// One mono input into both channels' comb banks and allpass chains.
    #[inline]
    pub fn process(&mut self, input: f32) -> [f32; 2] {
        let silent = input.abs() <= QUIET;
        if self.quiet >= SLEEP {
            if silent {
                return [0.0; 2];
            }
            self.quiet = 0;
        }
        let mut w = [0.0f32; 2];
        for c in 0..2 {
            for comb in self.comb[c].iter_mut() {
                w[c] += comb.process(input);
            }
            for ap in self.allpass[c].iter_mut() {
                w[c] = ap.process(w[c]);
            }
        }
        if silent && w[0].abs() <= QUIET && w[1].abs() <= QUIET {
            self.quiet += 1;
            if self.quiet == SLEEP {
                let q = self.quiet;
                self.clear();
                self.quiet = q;
            }
        } else {
            self.quiet = 0;
        }
        w
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reverb(mem: &mut std::vec::Vec<u8>) -> Combs<'_> {
        *mem = std::vec![0u8; Combs::need(44100.0, false) + 64];
        let mut pool = Pool::new(mem);
        let mut fv = Combs::new(44100.0, false, &mut pool).ok().unwrap();
        fv.set(0.8, 0.2);
        fv
    }

    #[test]
    fn a_silent_reverb_sleeps_and_wakes_with_the_input() {
        let (mut ma, mut mb) = (std::vec::Vec::new(), std::vec::Vec::new());
        let (mut a, mut b) = (reverb(&mut ma), reverb(&mut mb));
        // an impulse rings, decays far below a 16-bit step and then costs nothing
        let mut tail = 0;
        for i in 0..400_000 {
            let w = a.process(if i == 0 { 1.0 } else { 0.0 });
            if w[0].abs() > 1e-6 || w[1].abs() > 1e-6 {
                tail = i;
            }
        }
        assert!(tail > 20_000, "the tail rings for a while: {tail}");
        assert!(a.quiet >= SLEEP, "asleep");
        assert_eq!(a.process(0.0), [0.0, 0.0]);
        // asleep, it answers an impulse like a fresh reverb (the state it dropped was under −140 dBFS)
        let (mut worst, mut peak) = (0.0f32, 0.0f32);
        for i in 0..20_000 {
            let x = if i == 0 { 0.5 } else { 0.0 };
            let (wa, wb) = (a.process(x), b.process(x));
            worst = worst.max((wa[0] - wb[0]).abs()).max((wa[1] - wb[1]).abs());
            peak = peak.max(wb[0].abs());
        }
        assert!(peak > 0.01, "the reverb answers");
        assert!(worst < 1e-5 * peak.max(1.0), "{worst} against a peak of {peak}");
        assert_eq!(a.quiet, 0, "awake");
    }

    #[test]
    fn a_ringing_reverb_never_sleeps() {
        let mut mem = std::vec::Vec::new();
        let mut a = reverb(&mut mem);
        a.set(0.98, 0.0);
        a.process(1.0);
        for _ in 0..100_000 {
            a.process(0.0);
        }
        assert!(a.quiet < SLEEP, "a loud tail keeps it awake");
        assert!(a.process(0.0)[0] != 0.0 || a.process(0.0)[1] != 0.0);
    }
}
