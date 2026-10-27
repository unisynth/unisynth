//! The noise source (design D4): a per-voice stereo generator, uniform in ±1 and decorrelated left to right, seeded
//! at note-on from the voice's own stream. An optional band filter (Tunefish's: a ladder low-pass and an RBJ
//! high-pass at resonance 0.05) shapes it before it joins the oscillators.

use crate::flt::{self, Kind};
use crate::rng::Xorshift;
use synth_core::rng::Rng;

/// Resonance of the band filters.
pub const BAND_RES: f32 = 0.05;

/// Per-voice noise state.
#[derive(Clone, Copy, Debug)]
pub struct Noise {
    rng: Xorshift,
    pub lp: flt::Slot,
    pub hp: flt::Slot,
}

impl Noise {
    pub fn new() -> Self {
        Noise { rng: Xorshift::new(1), lp: flt::Slot::new(), hp: flt::Slot::new() }
    }

    /// Note-on: a fresh stream (the band filters keep their state, like the original filters).
    pub fn start(&mut self, seed: u32) {
        self.rng = Xorshift::new(seed);
    }

    /// One stereo frame at `level`; `band` runs it through the filters. `width` 1 gives independent channels, 0 the
    /// same noise on both (WaveSabre's), between them a mix with the power of each channel kept.
    #[inline]
    pub fn next(&mut self, level: f32, band: bool, width: f32) -> (f32, f32) {
        let l = (self.rng.next_f32() * 2.0 - 1.0) * level;
        let mut r = (self.rng.next_f32() * 2.0 - 1.0) * level;
        if width < 1.0 {
            let w = width.max(0.0);
            r = (w * r + (1.0 - w) * l) / crate::fm::sqrtf(w * w + (1.0 - w) * (1.0 - w));
        }
        if band {
            let (l, r) = self.lp.process(l, r);
            return self.hp.process(l, r);
        }
        (l, r)
    }
}

impl Default for Noise {
    fn default() -> Self {
        Self::new()
    }
}

/// The band filters' kinds.
pub const LP: Kind = Kind::Ladder;
pub const HP: Kind = Kind::RbjHp;

#[cfg(test)]
// helpers and imports of the tests that need a module this feature set lacks go unused
#[allow(dead_code, unused_imports)]
mod tests {
    use super::*;

    const RATE: f32 = 44100.0;

    #[test]
    fn the_generator_is_uniform_bipolar_and_decorrelated() {
        let mut n = Noise::new();
        n.start(5);
        let v: std::vec::Vec<(f32, f32)> = (0..20_000).map(|_| n.next(1.0, false, 1.0)).collect();
        assert!(v.iter().all(|(l, r)| l.abs() <= 1.0 && r.abs() <= 1.0));
        assert!(v.iter().any(|(l, _)| *l < -0.9) && v.iter().any(|(l, _)| *l > 0.9));
        assert!(v.iter().any(|(l, r)| l != r));
        let mean = v.iter().map(|p| p.0).sum::<f32>() / v.len() as f32;
        assert!(mean.abs() < 0.02);
    }

    #[test]
    fn the_width_runs_from_one_signal_on_both_channels_to_independent_ones_at_equal_power() {
        let corr_and_power = |w: f32| {
            let mut n = Noise::new();
            n.start(9);
            let v: std::vec::Vec<(f32, f32)> = (0..40_000).map(|_| n.next(1.0, false, w)).collect();
            let (mut lr, mut ll, mut rr) = (0.0f64, 0.0f64, 0.0f64);
            for (l, r) in &v {
                lr += (*l * *r) as f64;
                ll += (*l * *l) as f64;
                rr += (*r * *r) as f64;
            }
            (lr / (ll * rr).sqrt(), rr / ll)
        };
        let (c, p) = corr_and_power(0.0);
        assert!((c - 1.0).abs() < 1e-6 && (p - 1.0).abs() < 1e-6, "width 0 is mono: {c} {p}");
        let (c, p) = corr_and_power(1.0);
        assert!(c.abs() < 0.03 && (p - 1.0).abs() < 0.03, "width 1 is independent: {c} {p}");
        let (c, p) = corr_and_power(0.5);
        assert!(c > 0.5 && c < 0.9 && (p - 1.0).abs() < 0.03, "width 0.5 is in between at equal power: {c} {p}");
    }

    /// A patch that uses noise needs the feature: set, or routed from the matrix; unused noise costs nothing.
    #[cfg(feature = "us-env-timed")] // the default patch
    #[cfg(feature = "us-matrix")]
    #[test]
    fn the_noise_module_is_named_when_a_patch_uses_it_without_the_feature() {
        use crate::chunk::encode;
        use crate::law::host as inv;
        use crate::modmx::{source, target};
        use crate::params::{self as pm, m};
        let need = |edits: &[(usize, f32)]| {
            let mut v = pm::DEFAULTS;
            edits.iter().for_each(|&(i, x)| v[i] = x);
            crate::instrument::UInstrument::need(&encode(&v, &[]))
        };
        let set = [(pm::v::NOISE_LEVEL, 0.25)];
        let routed = [
            (pm::modslot(2, m::SOURCE), inv::inv_choice(source::VELOCITY, source::COUNT)),
            (pm::modslot(2, m::TARGET), inv::inv_index(target::NOISE_LEVEL)),
        ];
        assert!(need(&[]).is_ok(), "noise that is not used needs no module");
        for edits in [&set[..], &routed[..]] {
            match need(edits) {
                Ok(_) => assert!(cfg!(feature = "us-noise")),
                Err(e) => assert!(!cfg!(feature = "us-noise") && e == synth_core::mix::LoadError::UnsupportedModule(crate::module::NOISE)),
            }
        }
    }
}
