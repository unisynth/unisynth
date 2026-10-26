//! The mod matrix (design D6): 12 slots of `(source, target, amount, operation)`.
//!
//! A **Scale** slot contributes a factor; the factors aimed at one target multiply (`scale`). An **Offset** slot
//! contributes an addend in the target's physical unit; the addends add (`offset`). Whether a Scale factor multiplies
//! the normalized parameter or the physical value depends on the target ([`target::scales_physical`]).
//!
//! Slot result = `amount · source · own`, where `own` is the product of the Scale slots aimed at the slot's own amount
//! as it stood at the end of the previous pass (Tunefish's one-block self-modulation). Slots are evaluated in order,
//! reading the results already computed this pass for earlier slots and the previous pass's for later ones; an LFO
//! source is read with its depth modulated by what the matrix says so far.

use crate::law;
use crate::lfo;
use crate::params::{self as pm, m, COUNT};

/// Mod sources (`choice(p, 10)` of `m::SOURCE`).
pub mod source {
    pub const NONE: usize = 0;
    pub const ENV1: usize = 1;
    pub const LFO1: usize = 5;
    pub const WHEEL: usize = 7;
    pub const VELOCITY: usize = 8;
    pub const KEY: usize = 9;
    pub const COUNT: usize = 10;
}

/// Mod targets (`index(p)` of `m::TARGET`). Ids are frozen with chunk v1 (design D3).
pub mod target {
    pub const NONE: u8 = 0;
    /// All oscillators' pitch: Scale multiplies the frequency, Offset adds semitones.
    pub const PITCH: u8 = 1;
    /// Voice output gain (linear).
    pub const GAIN: u8 = 2;
    pub const PAN: u8 = 3;
    /// First oscillator target; slot `i` field `f` is `OSC + i·OSC_FIELDS + f`.
    pub const OSC: u8 = 4;
    pub const OSC_FIELDS: u8 = 15;
    pub const OSC_PITCH: u8 = 0;
    pub const OSC_GAIN: u8 = 1;
    pub const OSC_MIX: u8 = 2;
    pub const OSC_PAN: u8 = 3;
    pub const OSC_PM_OUT: u8 = 4;
    pub const OSC_FEEDBACK: u8 = 5;
    /// `OSC_SHAPE + k` is shape value `k` (0..9).
    pub const OSC_SHAPE: u8 = 6;
    pub const NOISE_LEVEL: u8 = 49;
    pub const NOISE_HP: u8 = 50;
    pub const NOISE_LP: u8 = 51;
    /// Filter slot `i`: cutoff `FILTER + 2i`, resonance `FILTER + 2i + 1`.
    pub const FILTER: u8 = 52;
    /// Envelope `i` decay.
    pub const ENV_DECAY: u8 = 60;
    /// LFO `i` depth.
    pub const LFO_DEPTH: u8 = 64;
    /// Mod slot `i`'s own amount.
    pub const SLOT: u8 = 66;
    pub const COUNT: u8 = 78;

    pub const fn osc(slot: usize, field: u8) -> u8 {
        OSC + slot as u8 * OSC_FIELDS + field
    }
    pub const fn filter_cutoff(slot: usize) -> u8 {
        FILTER + 2 * slot as u8
    }
    pub const fn filter_resonance(slot: usize) -> u8 {
        FILTER + 2 * slot as u8 + 1
    }

    /// Whether a Scale factor multiplies the physical value (frequency, linear gain, a slot's own amount) instead of
    /// the normalized parameter (cutoff, shapes, pan, …), as Tunefish's matrix does.
    pub const fn scales_physical(t: u8) -> bool {
        if t == PITCH || t == GAIN || t == NOISE_LEVEL || t >= SLOT {
            return true;
        }
        t >= OSC && t < NOISE_LEVEL && {
            let f = (t - OSC) % OSC_FIELDS;
            f == OSC_PITCH || f == OSC_GAIN
        }
    }

    /// The addend of an Offset slot at full amount, in the target's physical unit.
    pub const fn offset_range(t: u8) -> f32 {
        match t {
            PITCH => 36.0,
            GAIN | NOISE_LEVEL => 4.0,
            NOISE_HP | NOISE_LP => 19_980.0,
            _ if t >= OSC && t < NOISE_LEVEL => match (t - OSC) % OSC_FIELDS {
                OSC_PITCH => 36.0,
                OSC_GAIN | OSC_PM_OUT => 4.0,
                OSC_FEEDBACK => 8.0,
                _ => 1.0,
            },
            _ if t >= FILTER && t < ENV_DECAY && (t - FILTER) % 2 == 0 => 19_980.0,
            _ => 1.0,
        }
    }

    /// Readable name (host side).
    #[cfg(any(test, feature = "std"))]
    pub fn name(t: u8) -> std::string::String {
        use std::format;
        const OSC_NAMES: [&str; 15] =
            ["pitch", "gain", "mix", "pan", "pm_out", "feedback", "s0", "s1", "s2", "s3", "s4", "s5", "s6", "s7", "s8"];
        match t {
            NONE => "none".into(),
            PITCH => "pitch".into(),
            GAIN => "gain".into(),
            PAN => "pan".into(),
            _ if t < NOISE_LEVEL => format!("osc{}.{}", (t - OSC) / OSC_FIELDS + 1, OSC_NAMES[((t - OSC) % OSC_FIELDS) as usize]),
            NOISE_LEVEL => "noise.level".into(),
            NOISE_HP => "noise.hp".into(),
            NOISE_LP => "noise.lp".into(),
            _ if t < ENV_DECAY => format!("filt{}.{}", (t - FILTER) / 2 + 1, if (t - FILTER) % 2 == 0 { "cutoff" } else { "resonance" }),
            _ if t < LFO_DEPTH => format!("env{}.decay", t - ENV_DECAY + 1),
            _ if t < SLOT => format!("lfo{}.depth", t - LFO_DEPTH + 1),
            _ if t < COUNT => format!("mod{}.amount", t - SLOT + 1),
            _ => format!("target{t}"),
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct Slot {
    active: bool,
    scale: bool,
    target: u8,
    /// `amount · source · own` of the latest pass.
    res: f32,
}

const OFF: Slot = Slot { active: false, scale: true, target: 0, res: 1.0 };

/// Source values for one evaluation.
pub struct Sources {
    pub env: [f32; pm::ENV_SLOTS],
    /// LFO shape values `0..1` at the current phase.
    pub lfo_shape: [f32; pm::LFO_SLOTS],
    pub wheel: f32,
    pub velocity: f32,
    pub key: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct Matrix {
    slot: [Slot; pm::MOD_SLOTS],
    own: [f32; pm::MOD_SLOTS],
}

impl Matrix {
    pub const fn new() -> Self {
        Matrix { slot: [OFF; pm::MOD_SLOTS], own: [1.0; pm::MOD_SLOTS] }
    }

    /// Product of the Scale results aimed at `t` (1 when none).
    pub fn scale(&self, t: u8) -> f32 {
        let mut r = 1.0;
        for s in &self.slot {
            if s.active && s.scale && s.target == t {
                r *= s.res;
            }
        }
        r
    }

    /// Sum of the Offset results aimed at `t` (0 when none).
    pub fn offset(&self, t: u8) -> f32 {
        let mut r = 0.0;
        for s in &self.slot {
            if s.active && !s.scale && s.target == t {
                r += s.res;
            }
        }
        r
    }

    /// One pass over the slots with the current parameters.
    pub fn eval(&mut self, p: &[f32; COUNT], src: &Sources) {
        for i in 0..pm::MOD_SLOTS {
            let b = pm::modslot(i, 0);
            let source = law::choice(p[b + m::SOURCE], source::COUNT);
            let tgt = law::index(p[b + m::TARGET]);
            if source == source::NONE || tgt == target::NONE || tgt >= target::COUNT {
                self.slot[i] = OFF;
                continue;
            }
            let scale = law::choice(p[b + m::OPERATION], 2) == 0;
            let value = match source {
                s @ source::ENV1..=4 => src.env[s - source::ENV1],
                s @ (source::LFO1 | 6) => {
                    let k = s - source::LFO1;
                    let depth = p[pm::lfo(k, pm::l::DEPTH)] * self.scale(target::LFO_DEPTH + k as u8);
                    if scale {
                        lfo::scale_view(depth, src.lfo_shape[k])
                    } else {
                        lfo::offset_view(depth, src.lfo_shape[k])
                    }
                }
                source::WHEEL => src.wheel,
                source::VELOCITY => src.velocity,
                _ => src.key,
            };
            let amount = if scale { law::scale_amount(p[b + m::AMOUNT]) } else { law::offset_amount(p[b + m::AMOUNT], target::offset_range(tgt)) };
            self.slot[i] = Slot { active: true, scale, target: tgt, res: amount * value * self.own[i] };
        }
        for i in 0..pm::MOD_SLOTS {
            self.own[i] = self.scale(target::SLOT + i as u8);
        }
    }
}

/// What a control state asks of the matrix: the Scale product and Offset sum aimed at a target.
pub trait Query {
    fn scale(&self, t: u8) -> f32;
    fn offset(&self, t: u8) -> f32;
}

impl Query for Matrix {
    fn scale(&self, t: u8) -> f32 {
        Matrix::scale(self, t)
    }
    fn offset(&self, t: u8) -> f32 {
        Matrix::offset(self, t)
    }
}

/// Every target's Scale product and Offset sum, found in one pass over the slots: the voice asks about dozens of
/// targets per control segment, and scanning the slots for each costs more than the rest of the segment. The products
/// and sums are formed in slot order, so they equal [`Matrix::scale`] and [`Matrix::offset`] bit for bit.
pub struct Tab {
    s: [f32; target::COUNT as usize],
    o: [f32; target::COUNT as usize],
}

impl Query for Tab {
    #[inline]
    fn scale(&self, t: u8) -> f32 {
        self.s.get(t as usize).copied().unwrap_or(1.0)
    }
    #[inline]
    fn offset(&self, t: u8) -> f32 {
        self.o.get(t as usize).copied().unwrap_or(0.0)
    }
}

impl Matrix {
    /// The table of every target's Scale and Offset at the current slot results.
    pub fn tab(&self) -> Tab {
        let mut tab = Tab { s: [1.0; target::COUNT as usize], o: [0.0; target::COUNT as usize] };
        for s in self.slot.iter().filter(|s| s.active) {
            if let Some((x, y)) = tab.s.get_mut(s.target as usize).zip(tab.o.get_mut(s.target as usize)) {
                if s.scale {
                    *x *= s.res;
                } else {
                    *y += s.res;
                }
            }
        }
        tab
    }
}

impl Default for Matrix {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests;
