//! The multiplicative mod matrix (spec §3.6). Ten slots are stored in the patch; only the first 8 run. Each slot's
//! result is `amount · source · self`, where `self` is the slot's own "mod amount" target product from the
//! previous block. A destination's factor M is the product of the results of all slots aimed at it (1 if none).
//! Within a block, slots are evaluated in order; a slot that reads M (LFO depth, ADSR decay) sees the new results
//! of earlier slots and the previous block's results of later ones — the reference's one-block slot delay.

use crate::env::Adsr;
use crate::lfo::Lfo;
use crate::params::{ENV1, ENV2, LFO1, LFO2, MM_FIRST, mod_amount, mod_source, mod_target};

/// Live slots (of the 10 stored).
pub const LIVE: usize = 8;

/// Sources.
pub mod src {
    pub const LFO1: u8 = 1;
    pub const LFO2: u8 = 2;
    pub const ADSR1: u8 = 3;
    pub const ADSR2: u8 = 4;
    pub const WHEEL: u8 = 5;
}

/// Destinations (`round(39p)`).
pub mod dst {
    pub const BANDWIDTH: u8 = 1;
    pub const DAMP: u8 = 2;
    pub const HARMONICS: u8 = 3;
    pub const SCALE: u8 = 4;
    pub const VOLUME: u8 = 5;
    pub const FREQ: u8 = 6;
    pub const PAN: u8 = 7;
    pub const DETUNE: u8 = 8;
    pub const SPREAD: u8 = 9;
    pub const DRIVE: u8 = 10;
    pub const NOISE: u8 = 11;
    pub const LP_CUT: u8 = 12;
    pub const LP_RES: u8 = 13;
    pub const HP_CUT: u8 = 14;
    pub const HP_RES: u8 = 15;
    pub const BP_CUT: u8 = 16;
    pub const BP_Q: u8 = 17;
    pub const NT_CUT: u8 = 18;
    pub const NT_Q: u8 = 19;
    pub const ADSR1_DECAY: u8 = 20;
    pub const ADSR2_DECAY: u8 = 21;
    /// Slot 1's own amount; slot i at `SLOT1 + i`.
    pub const SLOT1: u8 = 22;
    pub const LFO1_DEPTH: u8 = 30;
    pub const LFO2_DEPTH: u8 = 31;
}

#[derive(Clone, Copy, Debug)]
struct Slot {
    dst: u8,
    result: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct ModMatrix {
    slots: [Slot; LIVE],
    /// Each slot's own-amount factor from the previous block.
    own: [f32; LIVE],
    pub env: [Adsr; 2],
    pub lfo: [Lfo; 2],
}

impl ModMatrix {
    pub const fn new() -> Self {
        ModMatrix {
            slots: [Slot { dst: 0, result: 1.0 }; LIVE],
            own: [1.0; LIVE],
            env: [Adsr::new(); 2],
            lfo: [Lfo { phase: 0.0 }; 2],
        }
    }

    /// M(destination).
    pub fn get(&self, d: u8) -> f32 {
        self.slots.iter().filter(|s| s.dst == d).fold(1.0, |m, s| m * s.result)
    }

    pub fn note_on(&mut self, lfo_phase: [f32; 2]) {
        self.env.iter_mut().for_each(Adsr::trigger);
        self.lfo[0].phase = lfo_phase[0];
        self.lfo[1].phase = lfo_phase[1];
    }

    pub fn note_off(&mut self) {
        self.env.iter_mut().for_each(Adsr::release);
    }

    /// One block. Returns whether an envelope used as a source was still running at the start of the block.
    pub fn step(&mut self, p: &[f32], wheel: f32) -> bool {
        let mut values = [0.0f32; 6];
        let mut done = [false; 6];
        let mut env_running = false;
        for i in 0..LIVE {
            let b = MM_FIRST + 3 * i;
            let (s, amount) = (mod_source(p[b]), mod_amount(p[b + 1]));
            self.slots[i] = Slot { dst: mod_target(p[b + 2]), result: 1.0 };
            let s = s as usize;
            if !(1..=5).contains(&s) {
                continue;
            }
            if !done[s] {
                done[s] = true;
                values[s] = match s as u8 {
                    src::LFO1 => self.lfo[0].step(&p[LFO1..LFO1 + 3], self.get(dst::LFO1_DEPTH)),
                    src::LFO2 => self.lfo[1].step(&p[LFO2..LFO2 + 3], self.get(dst::LFO2_DEPTH)),
                    src::ADSR1 | src::ADSR2 => {
                        let k = s - src::ADSR1 as usize;
                        env_running |= !self.env[k].done();
                        let (base, d) = if k == 0 { (ENV1, dst::ADSR1_DECAY) } else { (ENV2, dst::ADSR2_DECAY) };
                        let m = self.get(d);
                        self.env[k].step(&p[base..base + 5], m)
                    }
                    _ => wheel,
                };
            }
            self.slots[i].result = amount * values[s] * self.own[i];
        }
        for i in 0..LIVE {
            self.own[i] = self.get(dst::SLOT1 + i as u8);
        }
        env_running
    }
}

impl Default for ModMatrix {
    fn default() -> Self {
        Self::new()
    }
}
