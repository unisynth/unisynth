//! Per-voice stereo noise (spec §3.2): the shared looped white-noise table read from two random offsets, level
//! `NoiseOn · velocity · M(noise)` (silent at ≤ 0.01), optionally band-passed by its own ladder low-pass at
//! `freq + bw` and RBJ high-pass at `freq − bw` (resonance 0.05). The noise stage overwrites the voice buffer.

use crate::filter::{Ladder, Rbj, RbjKind};
use crate::params::{NOISE_AMOUNT, NOISE_BW, NOISE_FREQ};
use crate::tables::{tables, NOISE_LEN};

#[derive(Clone, Copy, Debug, Default)]
pub struct Noise {
    pub offset: [usize; 2],
    filtered: bool,
    amount: f32,
    lp: Ladder,
    hp: Rbj,
}

impl Noise {
    pub fn update(&mut self, p: &[f32], velocity: f32, m: f32) {
        let bw = p[NOISE_BW];
        self.filtered = bw < 0.99;
        self.amount = p[NOISE_AMOUNT] * velocity * m;
        if self.filtered && self.amount > 0.0 {
            let f = p[NOISE_FREQ];
            self.hp.set(RbjKind::HighPass, f - bw, 0.05);
            self.lp.set(f + bw, 0.05);
        }
    }

    pub fn run(&mut self, l: &mut [f32], r: &mut [f32]) {
        if self.amount <= 0.01 {
            l.fill(0.0);
            r.fill(0.0);
            return;
        }
        let white = &tables().white;
        for (c, buf) in [&mut *l, &mut *r].into_iter().enumerate() {
            for v in buf.iter_mut() {
                *v = white[self.offset[c]] * self.amount;
                self.offset[c] = (self.offset[c] + 1) % NOISE_LEN;
            }
        }
        if self.filtered {
            self.lp.run(l, r);
            self.hp.run(l, r);
        }
    }
}
