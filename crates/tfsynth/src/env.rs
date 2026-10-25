//! ADSR envelope (spec §3.4), advanced once per 512-frame block. Log-shaped per-block steps, a slope-controlled
//! decay/release curve, and the reference's quirks: decay can rise when the mod matrix pushes it above ≈1.02,
//! and sustain chases a changed level by the decay step.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stage {
    Attack,
    Decay,
    Sustain,
    Release,
    Finished,
}

#[derive(Clone, Copy, Debug)]
pub struct Adsr {
    pub stage: Stage,
    pub value: f32,
}

/// Per-block step scale `0.0005 · N` at 44.1 kHz (N = 512).
pub const STEP: f32 = 0.0005 * 512.0;

impl Adsr {
    pub const fn new() -> Self {
        Adsr { stage: Stage::Finished, value: 0.0 }
    }

    pub fn trigger(&mut self) {
        self.stage = Stage::Attack;
        self.value = 0.0;
    }

    pub fn release(&mut self) {
        self.stage = Stage::Release;
    }

    pub fn done(&self) -> bool {
        self.stage == Stage::Finished
    }

    /// One block. `p` = the five patch values (attack, decay, sustain, release, slope); `decay_mod` = M(decay).
    pub fn step(&mut self, p: &[f32], decay_mod: f32) -> f32 {
        let (a, d, s, r, slope) = (p[0], p[1], p[2], p[3], p[4]);
        let cube = |x: f32| libm::pow(x as f64, 3.0) as f32;
        let attack = -libm::log10f(cube(a).max(1e-9) * 0.94) * STEP;
        let decay = libm::log10f(cube(d * decay_mod).max(1e-9) * 0.94) * 0.25 * STEP;
        let sustain = s.min(0.99);
        let release = libm::log10f(cube(r).max(1e-9) * 0.94) * 0.25 * STEP;
        let mut v = self.value;
        match self.stage {
            Stage::Attack => {
                v += attack;
                if v >= 1.0 {
                    v = 1.0;
                    self.stage = Stage::Decay;
                }
            }
            Stage::Decay => {
                if v < sustain {
                    v = sustain;
                    self.stage = Stage::Sustain;
                } else {
                    let pos = (0.01 + (v - sustain)) / (1.0 - sustain);
                    v += decay * libm::pow(pos as f64, slope as f64) as f32;
                    if v <= sustain {
                        v = sustain;
                        self.stage = Stage::Sustain;
                    }
                }
            }
            Stage::Sustain => {
                if v < sustain {
                    v -= decay;
                    if v > sustain {
                        v = sustain;
                    }
                } else if v > sustain {
                    v += decay;
                    if v < sustain {
                        v = sustain;
                    }
                }
            }
            Stage::Release => {
                v += release * libm::pow(v as f64, slope as f64) as f32;
                if v <= 1e-5 {
                    v = 0.0;
                    self.stage = Stage::Finished;
                }
            }
            Stage::Finished => {}
        }
        self.value = v;
        v
    }
}

impl Default for Adsr {
    fn default() -> Self {
        Self::new()
    }
}
