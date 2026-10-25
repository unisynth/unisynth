//! Reverb (spec §4.3): Freeverb topology (Jezar, public domain) on `synth-core`'s comb/allpass — 8 parallel damped
//! combs into 4 series allpasses per channel, stock 44.1 kHz lengths; both channels use the same comb lengths, only
//! the right allpasses are 23 samples longer. Input `(L + R)·0.015`; feedback `0.7 + 0.28·room`, damping
//! `0.4·damp`, allpass feedback 0.5; wet `3·wet` split by width, dry `2·(1 − wet)`.

use crate::params::{COUNT, REV_DAMP, REV_ROOM, REV_WET, REV_WIDTH};
use synth_core::dsp::{AllPass, Comb};
use synth_core::pool::{OutOfMemory, Pool};

/// Freeverb's published comb and allpass tunings at 44.1 kHz, and its stereo spread.
pub const COMBS: [usize; 8] = [1116, 1188, 1277, 1356, 1422, 1491, 1557, 1617];
pub const ALLPASSES: [usize; 4] = [556, 441, 341, 225];
pub const SPREAD: usize = 23;

pub struct Reverb<'a> {
    comb: [[Comb<'a>; 8]; 2],
    allpass: [[AllPass<'a>; 4]; 2],
}

fn build<'p, T, const N: usize>(
    pool: &mut Pool<'p>,
    lens: [usize; N],
    extra: usize,
    make: impl Fn(&'p mut [f32]) -> T,
) -> Result<[T; N], OutOfMemory> {
    let mut out: [Option<T>; N] = core::array::from_fn(|_| None);
    for (o, &n) in out.iter_mut().zip(lens.iter()) {
        *o = Some(make(pool.slice(n + extra)?));
    }
    Ok(out.map(|o| o.unwrap()))
}

impl<'a> Reverb<'a> {
    pub fn need() -> usize {
        let c: usize = COMBS.iter().map(|&n| 2 * Pool::need::<f32>(n)).sum();
        let a: usize = ALLPASSES.iter().map(|&n| Pool::need::<f32>(n) + Pool::need::<f32>(n + SPREAD)).sum();
        c + a
    }

    pub fn new(pool: &mut Pool<'a>) -> Result<Self, OutOfMemory> {
        let comb = [build(pool, COMBS, 0, Comb::new)?, build(pool, COMBS, 0, Comb::new)?];
        let mut allpass = [build(pool, ALLPASSES, 0, AllPass::new)?, build(pool, ALLPASSES, SPREAD, AllPass::new)?];
        allpass.iter_mut().flatten().for_each(|a| a.feedback = 0.5);
        Ok(Reverb { comb, allpass })
    }

    pub fn clear(&mut self) {
        self.comb.iter_mut().flatten().for_each(|c| c.clear());
        self.allpass.iter_mut().flatten().for_each(|a| a.clear());
    }

    pub fn run(&mut self, p: &[f32; COUNT], l: &mut [f32], r: &mut [f32]) {
        let feedback = p[REV_ROOM] * 0.28 + 0.7;
        let damp = p[REV_DAMP] * 0.4;
        let wet = p[REV_WET] * 3.0;
        let dry = (1.0 - p[REV_WET]) * 2.0;
        let width = p[REV_WIDTH];
        let (wet1, wet2) = (wet * (width / 2.0 + 0.5), wet * ((1.0 - width) / 2.0));
        self.comb.iter_mut().flatten().for_each(|c| {
            c.feedback = feedback;
            c.damp = damp;
        });
        for (lv, rv) in l.iter_mut().zip(r.iter_mut()) {
            let input = (*lv + *rv) * 0.015;
            let mut w = [0.0f32; 2];
            for c in 0..2 {
                for comb in self.comb[c].iter_mut() {
                    w[c] += comb.process(input);
                }
                for ap in self.allpass[c].iter_mut() {
                    w[c] = ap.process(w[c]);
                }
            }
            *lv = w[0] * wet1 + w[1] * wet2 + *lv * dry;
            *rv = w[1] * wet1 + w[0] * wet2 + *rv * dry;
        }
    }
}
