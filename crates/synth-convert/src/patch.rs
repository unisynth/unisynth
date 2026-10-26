//! A unified instrument patch under construction: the 177 normalized values (design D3) with setters in physical
//! terms, the mod-matrix slot allocator, and the chunk encoder. Values are clamped into `0..1` (sampler indices stay
//! raw) and non-finite ones fall back to the default, so a hostile legacy chunk cannot produce an invalid unified one.

use unisynth::law::host as inv;
use unisynth::modmx::{source, target};
use unisynth::params::{self as pm, e, f, l, m, o, osc_type, shape, COUNT, DEFAULTS};

#[derive(Clone, Debug)]
pub struct Patch {
    pub v: [f32; COUNT],
    slots: usize,
}

/// Mod slot operation.
#[derive(Clone, Copy, PartialEq)]
pub enum Op {
    Scale,
    Offset,
}

impl Patch {
    pub fn new() -> Patch {
        Patch { v: DEFAULTS, slots: 0 }
    }

    pub fn set(&mut self, i: usize, p: f32) {
        self.v[i] = p;
    }

    pub fn osc(&mut self, slot: usize, fld: usize, p: f32) {
        self.v[pm::osc(slot, fld)] = p;
    }

    pub fn shape(&mut self, slot: usize, k: usize, p: f32) {
        self.v[pm::osc(slot, o::SHAPE + k)] = p;
    }

    pub fn osc_type(&mut self, slot: usize, t: usize) {
        self.osc(slot, o::TYPE, inv::inv_choice(t, osc_type::COUNT));
    }

    /// Sampler slot source (0 GSM, 1 DLS) and its raw index (1-based, 0 = none).
    pub fn sampler_source(&mut self, slot: usize, dls: bool, index: f32) {
        self.shape(slot, shape::SMP_SOURCE, inv::inv_choice(if dls { pm::smp_source::WAVE } else { pm::smp_source::GSM }, pm::smp_source::COUNT));
        self.v[pm::smp_index(slot)] = index;
    }

    pub fn filter(&mut self, slot: usize, kind: usize, cutoff: f32, resonance: f32) {
        self.v[pm::filt(slot, f::TYPE)] = inv::inv_choice(kind, pm::filter_type::COUNT);
        self.v[pm::filt(slot, f::CUTOFF)] = cutoff;
        self.v[pm::filt(slot, f::RESONANCE)] = resonance;
    }

    /// Envelope `slot` (timed mode): attack, decay, release as the legacy 0..1 parameters (the law is the same),
    /// sustain direct.
    pub fn env(&mut self, slot: usize, a: f32, d: f32, s: f32, r: f32) {
        self.v[pm::env(slot, e::ATTACK)] = a;
        self.v[pm::env(slot, e::DECAY)] = d;
        self.v[pm::env(slot, e::SUSTAIN)] = s;
        self.v[pm::env(slot, e::RELEASE)] = r;
    }

    /// LFO `slot`: sine, rate in Hz, full depth, restarting at every note.
    pub fn lfo_sine(&mut self, slot: usize, hz: f32) {
        self.v[pm::lfo(slot, l::SHAPE)] = inv::inv_choice(unisynth::lfo::shape::SINE, unisynth::lfo::shape::COUNT);
        self.v[pm::lfo(slot, l::RATE)] = inv::inv_lfo_rate(hz);
        self.v[pm::lfo(slot, l::DEPTH)] = 1.0;
        self.v[pm::lfo(slot, l::SYNC)] = 1.0;
    }

    /// Adds a mod slot: `src` is a `source::*` id, `tgt` a `target::*` id, `amount_p` the normalized amount.
    pub fn route(&mut self, src: usize, tgt: u8, amount_p: f32, op: Op) {
        let s = self.slots;
        assert!(s < pm::MOD_SLOTS, "more than {} mod slots", pm::MOD_SLOTS);
        self.slots += 1;
        self.v[pm::modslot(s, m::SOURCE)] = inv::inv_choice(src, source::COUNT);
        self.v[pm::modslot(s, m::TARGET)] = inv::inv_index(tgt);
        self.v[pm::modslot(s, m::AMOUNT)] = amount_p;
        self.v[pm::modslot(s, m::OPERATION)] = inv::inv_choice((op == Op::Offset) as usize, 2);
    }

    /// Leaves the next mod slot unused, so later slots keep their index.
    pub fn skip(&mut self) {
        assert!(self.slots < pm::MOD_SLOTS);
        self.slots += 1;
    }

    /// A Scale slot with factor `x` (the slot's `amount`).
    pub fn scale(&mut self, src: usize, tgt: u8, x: f32) {
        self.route(src, tgt, inv::inv_scale_amount(x), Op::Scale);
    }

    /// An Offset slot adding `x` (in the target's unit, within its range) at full source.
    pub fn offset(&mut self, src: usize, tgt: u8, x: f32) {
        self.route(src, tgt, inv::inv_offset_amount(x, target::offset_range(tgt)), Op::Offset);
    }

    /// The values after the sanity pass.
    pub fn finish(mut self) -> [f32; COUNT] {
        for (i, x) in self.v.iter_mut().enumerate() {
            if !x.is_finite() {
                *x = DEFAULTS[i];
            } else if !pm::is_raw(i) {
                *x = x.clamp(0.0, 1.0);
            } else {
                *x = x.clamp(0.0, 65535.0);
            }
        }
        self.v
    }
}

impl Default for Patch {
    fn default() -> Self {
        Self::new()
    }
}
