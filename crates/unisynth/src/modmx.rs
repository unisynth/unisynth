//! The mod matrix (design D6): 16 slots of `(source, target, amount, operation)` (slots 13–16 and the Add operation:
//! change add-v2-import, design D3).
//!
//! A **Scale** slot contributes a factor; the factors aimed at one target multiply (`scale`). An **Offset** slot
//! contributes an addend in the target's physical unit; the addends add (`offset`). Whether a Scale factor multiplies
//! the normalized parameter or the physical value depends on the target ([`target::scales_physical`]).
//!
//! Slot result = `amount · source · own`, where `own` is the product of the Scale slots aimed at the slot's own amount
//! as it stood at the end of the previous pass (Tunefish's one-block self-modulation). Slots are evaluated in order,
//! reading the results already computed this pass for earlier slots and the previous pass's for later ones; an LFO
//! source is read with its depth modulated by what the matrix says so far.
//!
//! An **Add** slot (V2's rule) adds `amount · source` (amount `−1..1`) to the normalized value of a parameter target
//! before the parameter is read, clamping to `0..1` after each slot in slot order ([`Matrix::apply`]).

use crate::law;
use crate::lfo;
use crate::params::{self as pm, m, COUNT};

/// Mod sources (`m::SOURCE`, stored as an expanded choice: `params::kinds` with 10 old sources).
pub mod source {
    pub const NONE: usize = 0;
    pub const ENV1: usize = 1;
    pub const LFO1: usize = 5;
    pub const WHEEL: usize = 7;
    pub const VELOCITY: usize = 8;
    pub const KEY: usize = 9;
    /// Sources of chunk v1 (`inv_choice(s, COUNT)` still encodes them).
    pub const COUNT: usize = 10;
    /// Controller 2 (breath); controllers 2–7 are `CC2..CC2 + 6` (change add-v2-import).
    pub const CC2: usize = 10;
    /// V2's note source `2·(note − 48)/128`.
    pub const V2_NOTE: usize = 16;
    pub const ALL: usize = 17;
    #[inline]
    pub fn decode(p: f32) -> usize {
        crate::params::kinds::decode_in(p, COUNT, ALL - COUNT, NONE, cfg!(feature = "us-matrix-add"))
    }
    pub fn encode(id: usize) -> f32 {
        crate::params::kinds::encode(id, COUNT)
    }
}

/// Slot operations (`m::OPERATION`, an expanded choice with 2 old operations).
pub mod op {
    pub const SCALE: usize = 0;
    pub const OFFSET: usize = 1;
    pub const COUNT: usize = 2;
    /// V2's additive rule (change add-v2-import).
    pub const ADD: usize = 2;
    pub const ALL: usize = 3;
    #[inline]
    pub fn decode(p: f32) -> usize {
        crate::params::kinds::decode_in(p, COUNT, ALL - COUNT, SCALE, cfg!(feature = "us-matrix-add"))
    }
    pub fn encode(id: usize) -> f32 {
        crate::params::kinds::encode(id, COUNT)
    }
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
    /// Mod slot `i`'s own amount (slots 1–12).
    pub const SLOT: u8 = 66;
    /// Parameter targets (Add slots only): `PARAM + k` is parameter [`PARAMS`]`[k]`.
    pub const PARAM: u8 = 78;
    pub const COUNT: u8 = PARAM + PARAMS.len() as u8;

    use crate::params::{self as pm, ch, dist, e, l, lx, o, shape, v, v2};
    /// The parameters an Add slot can target: the 64 V2 parameters `sounddef.h` marks as modulation destinations, in
    /// V2's order, at their unified index (design D4a). Frozen with this table.
    pub const PARAMS: [usize; 64] = [
        v::PAN,
        v::TRANSPOSE,
        pm::osc(0, o::COARSE),
        pm::osc(0, o::FINE),
        pm::osc(0, o::SHAPE + shape::V2_COLOR),
        pm::osc(0, o::GAIN),
        pm::osc(1, o::COARSE),
        pm::osc(1, o::FINE),
        pm::osc(1, o::SHAPE + shape::V2_COLOR),
        pm::osc(1, o::GAIN),
        pm::osc(2, o::COARSE),
        pm::osc(2, o::FINE),
        pm::osc(2, o::SHAPE + shape::V2_COLOR),
        pm::osc(2, o::GAIN),
        pm::filt(0, pm::f::CUTOFF),
        pm::filt(0, pm::f::RESONANCE),
        pm::filt(1, pm::f::CUTOFF),
        pm::filt(1, pm::f::RESONANCE),
        v2::BALANCE,
        pm::VDIST + dist::GAIN,
        pm::VDIST + dist::P1,
        pm::VDIST + dist::P2,
        pm::env(0, e::ATTACK),
        pm::env(0, e::DECAY),
        pm::env(0, e::SUSTAIN),
        pm::env(0, e::SLOPE),
        pm::env(0, e::RELEASE),
        pm::env_amp(0),
        pm::env(1, e::ATTACK),
        pm::env(1, e::DECAY),
        pm::env(1, e::SUSTAIN),
        pm::env(1, e::SLOPE),
        pm::env(1, e::RELEASE),
        pm::env_amp(1),
        pm::lfo(0, l::RATE),
        pm::lfo_ext(0, lx::PHASE),
        pm::lfo(0, l::DEPTH),
        pm::lfo(1, l::RATE),
        pm::lfo_ext(1, lx::PHASE),
        pm::lfo(1, l::DEPTH),
        ch::VOL,
        ch::RECV_AUXA,
        ch::RECV_AUXB,
        ch::SEND_AUXA,
        ch::SEND_AUXB,
        ch::SEND_RVB,
        ch::SEND_DLY,
        ch::BOOST,
        ch::DIST + dist::GAIN,
        ch::DIST + dist::P1,
        ch::DIST + dist::P2,
        ch::CHORUS,
        ch::CHORUS + 1,
        ch::CHORUS + 2,
        ch::CHORUS + 3,
        ch::CHORUS + 4,
        ch::CHORUS + 5,
        ch::CHORUS + 6,
        ch::COMP + 3,
        ch::COMP + 4,
        ch::COMP + 5,
        ch::COMP + 6,
        ch::COMP + 7,
        ch::COMP + 8,
    ];

    /// The parameter a parameter target names.
    pub const fn param(t: u8) -> Option<usize> {
        if t >= PARAM && t < COUNT {
            Some(PARAMS[(t - PARAM) as usize])
        } else {
            None
        }
    }

    /// Whether parameter target `t` belongs to the channel stage (evaluated from the newest voice, design D3).
    pub const fn is_channel(t: u8) -> bool {
        match param(t) {
            Some(i) => i >= pm::CH,
            None => false,
        }
    }

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
            _ if t < PARAM => format!("mod{}.amount", t - SLOT + 1),
            _ if t < COUNT => format!("param.{}", crate::params::name(PARAMS[(t - PARAM) as usize])),
            _ => format!("target{t}"),
        }
    }
}

/// Slots this build evaluates: 16 with `us-matrix-add`, else the 12 of chunk v1.
pub const SLOTS: usize = if cfg!(feature = "us-matrix-add") { pm::MOD_SLOTS } else { pm::MOD_SLOTS1 };

/// Whether slot `i` needs `us-matrix-add`: it routes and is one of slots 13–16, Adds, or reads a later source.
pub fn needs_add(p: &[f32; COUNT], i: usize) -> bool {
    let b = pm::modslot(i, 0);
    routes(p, i) && (i >= pm::MOD_SLOTS1 || op::decode(p[b + m::OPERATION]) == op::ADD || source::decode(p[b + m::SOURCE]) >= source::COUNT)
}

/// Whether mod slot `i` routes: it has a source and a valid target (what `Matrix::eval` makes active).
pub fn routes(p: &[f32; COUNT], i: usize) -> bool {
    let b = pm::modslot(i, 0);
    let t = law::index(p[b + m::TARGET]);
    let add = op::decode(p[b + m::OPERATION]) == op::ADD;
    // an Add slot acts only on a parameter target, the others only on the targets before them
    source::decode(p[b + m::SOURCE]) != source::NONE && t != target::NONE && t < target::COUNT && (add == (t >= target::PARAM))
}

#[derive(Clone, Copy, Debug)]
struct Slot {
    active: bool,
    scale: bool,
    /// An Add slot (`scale` is then false and the slot is not an Offset).
    add: bool,
    /// The source reads the voice (an envelope, an LFO or the note): V2 before version 5 skips those in channel routes.
    voice_src: bool,
    target: u8,
    /// `amount · source · own` of the latest pass.
    res: f32,
}

const OFF: Slot = Slot { active: false, scale: true, add: false, voice_src: false, target: 0, res: 1.0 };

/// Source values for one evaluation.
pub struct Sources {
    pub env: [f32; pm::ENV_SLOTS],
    /// LFO shape values `0..1` at the current phase.
    pub lfo_shape: [f32; pm::LFO_SLOTS],
    /// What an Add slot reads from each LFO (`depth · shape`, or a V2-mode LFO's output / 128 with its polarity).
    pub lfo_add: [f32; pm::LFO_SLOTS],
    pub wheel: f32,
    pub velocity: f32,
    pub key: f32,
    /// Controllers 2–7 (`0..1`).
    pub cc: [f32; pm::CC_COUNT],
    /// The note number (V2's note source is `2·(note − 48)/128`).
    pub note: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct Matrix {
    slot: [Slot; SLOTS],
    own: [f32; pm::MOD_SLOTS1],
}

impl Matrix {
    pub const fn new() -> Self {
        Matrix { slot: [OFF; SLOTS], own: [1.0; pm::MOD_SLOTS1] }
    }

    /// Product of the Scale results aimed at `t` (1 when none).
    pub fn scale(&self, t: u8) -> f32 {
        if !cfg!(feature = "us-matrix") {
            return 1.0;
        }
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
        if !cfg!(feature = "us-matrix") {
            return 0.0;
        }
        let mut r = 0.0;
        for s in &self.slot {
            if s.active && !s.scale && !s.add && s.target == t {
                r += s.res;
            }
        }
        r
    }

    /// One pass over the slots with the current parameters.
    /// Without `us-matrix` no patch with a route loads (`instrument::check_voice`): every slot stays off.
    pub fn eval(&mut self, p: &[f32; COUNT], src: &Sources) {
        if !cfg!(feature = "us-matrix") {
            return;
        }
        for i in 0..SLOTS {
            // without `us-matrix-add` no patch that needs it loads (`instrument::check_voice`), so the slots here never
            // Add or read a later source
            if !routes(p, i) {
                self.slot[i] = OFF;
                continue;
            }
            let b = pm::modslot(i, 0);
            let source = source::decode(p[b + m::SOURCE]);
            let tgt = law::index(p[b + m::TARGET]);
            let opn = op::decode(p[b + m::OPERATION]);
            let (scale, add) = (opn == op::SCALE, opn == op::ADD);
            let value = match source {
                s @ source::ENV1..=4 => src.env[s - source::ENV1],
                s @ (source::LFO1 | 6) => {
                    let k = s - source::LFO1;
                    if add {
                        src.lfo_add[k]
                    } else {
                        let depth = p[pm::lfo(k, pm::l::DEPTH)] * self.scale(target::LFO_DEPTH + k as u8);
                        if scale {
                            lfo::scale_view(depth, src.lfo_shape[k])
                        } else {
                            lfo::offset_view(depth, src.lfo_shape[k])
                        }
                    }
                }
                source::WHEEL => src.wheel,
                source::VELOCITY => src.velocity,
                source::KEY => src.key,
                s @ source::CC2..=15 if cfg!(feature = "us-matrix-add") => src.cc[s - source::CC2],
                _ if cfg!(feature = "us-matrix-add") => 2.0 * (src.note - 48.0) / 128.0,
                _ => src.key,
            };
            let amount = if add {
                p[b + m::AMOUNT] * 2.0 - 1.0
            } else if scale {
                law::scale_amount(p[b + m::AMOUNT])
            } else {
                law::offset_amount(p[b + m::AMOUNT], target::offset_range(tgt))
            };
            let own = if i < pm::MOD_SLOTS1 { self.own[i] } else { 1.0 };
            let voice_src = matches!(source, source::ENV1..=6 | source::KEY | source::V2_NOTE);
            self.slot[i] = Slot { active: true, scale, add, voice_src, target: tgt, res: amount * value * own };
        }
        for i in 0..pm::MOD_SLOTS1 {
            self.own[i] = self.scale(target::SLOT + i as u8);
        }
    }

    /// Whether any Add slot is active.
    pub fn has_adds(&self) -> bool {
        cfg!(feature = "us-matrix-add") && self.slot.iter().any(|s| s.active && s.add)
    }

    /// Applies the Add slots to `v` (a copy of the parameters) in slot order, clamping each step to `0..1` (V2's
    /// `clamp(v + a·s, 0, 128)`). `channel` picks the channel-stage targets (true) or the voice targets (false).
    pub fn apply(&self, v: &mut [f32; COUNT], channel: bool) {
        self.apply_from(v, channel, true);
    }

    /// [`Matrix::apply`]; `voice_src` false leaves out the slots that read an envelope, an LFO or the note.
    pub fn apply_from(&self, v: &mut [f32; COUNT], channel: bool, voice_src: bool) {
        if !cfg!(feature = "us-matrix-add") {
            return;
        }
        for s in self.slot.iter().filter(|s| s.active && s.add && target::is_channel(s.target) == channel && (voice_src || !s.voice_src)) {
            if let Some(i) = target::param(s.target) {
                v[i] = (v[i] + s.res).max(0.0).min(1.0);
            }
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
    s: [f32; target::PARAM as usize],
    o: [f32; target::PARAM as usize],
}

impl Query for Tab {
    #[inline]
    fn scale(&self, t: u8) -> f32 {
        if !cfg!(feature = "us-matrix") {
            return 1.0;
        }
        self.s.get(t as usize).copied().unwrap_or(1.0)
    }
    #[inline]
    fn offset(&self, t: u8) -> f32 {
        if !cfg!(feature = "us-matrix") {
            return 0.0;
        }
        self.o.get(t as usize).copied().unwrap_or(0.0)
    }
}

impl Matrix {
    /// The table of every target's Scale and Offset at the current slot results.
    pub fn tab(&self) -> Tab {
        let mut tab = Tab { s: [1.0; target::PARAM as usize], o: [0.0; target::PARAM as usize] };
        for s in self.slot.iter().filter(|s| s.active && !s.add) {
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

#[cfg(all(test, feature = "us-matrix"))]
mod tests;
