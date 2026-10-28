//! Filter slots (design D4): SVF low/high/band/notch (WaveSabre's Chamberlin filter, 2× oversampled), the 4-pole
//! ladder low-pass, RBJ high/band-pass and the resonator notch (Tunefish). All take the cutoff in Hz (the unified law
//! is applied by the caller) and compute their coefficients for the song rate; a segment's coefficients ramp linearly
//! from the previous segment's, so a cutoff sweep has no steps. Resonance keeps each type's original law:
//! SVF `q = 1 − p`; the others clamp `p` to `0..=0.85`.

use crate::module;
use crate::params::filter_type as ft;
use crate::ramp::Ramp;
use synth_core::mix::LoadError;

/// Coefficients per filter (the biquads use all five).
pub const NC: usize = 5;
/// State floats per channel (the ladder uses eight).
const NS: usize = 9;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Kind {
    #[default]
    Off,
    SvfLp,
    SvfHp,
    SvfBp,
    SvfNotch,
    Ladder,
    RbjHp,
    RbjBp,
    Notch,
    /// V2's LRC filter (2× oversampled) in a `v2::fmode` (low, band, high, notch, all-pass); cutoff and resonance are
    /// V2 raw / 128 ([`coefs_v2`]). Only in builds with `us-flt-v2` (the payload would cost every match).
    #[cfg(feature = "us-flt-v2")]
    V2(u8),
}

impl Kind {
    /// The type for a `filter_type` value, or the module that is missing from this build.
    pub fn check(ty: usize) -> Result<Kind, LoadError> {
        let (kind, id, on) = match ty {
            ft::SVF_LP => (Kind::SvfLp, module::FILTER_SVF, cfg!(feature = "us-flt-svf")),
            ft::SVF_HP => (Kind::SvfHp, module::FILTER_SVF, cfg!(feature = "us-flt-svf")),
            ft::SVF_BP => (Kind::SvfBp, module::FILTER_SVF, cfg!(feature = "us-flt-svf")),
            ft::SVF_NOTCH => (Kind::SvfNotch, module::FILTER_SVF, cfg!(feature = "us-flt-svf")),
            ft::LADDER_LP => (Kind::Ladder, module::FILTER_LADDER, cfg!(feature = "us-flt-ladder")),
            ft::RBJ_HP => (Kind::RbjHp, module::FILTER_RBJ, cfg!(feature = "us-flt-rbj")),
            ft::RBJ_BP => (Kind::RbjBp, module::FILTER_RBJ, cfg!(feature = "us-flt-rbj")),
            ft::NOTCH => (Kind::Notch, module::FILTER_NOTCH, cfg!(feature = "us-flt-notch")),
            #[cfg(feature = "us-flt-v2")]
            ft::V2_LP..=ft::V2_MOOG_HP => (Kind::V2((ty - ft::V2_LP) as u8 + 1), module::FILTER_V2, true),
            #[cfg(not(feature = "us-flt-v2"))]
            ft::V2_LP..=ft::V2_MOOG_HP => return Err(LoadError::UnsupportedModule(module::FILTER_V2)),
            _ => return Ok(Kind::Off),
        };
        if on { Ok(kind) } else { Err(LoadError::UnsupportedModule(id)) }
    }

    /// At run time: a type automated to a module this build lacks bypasses (the load already rejected the patch's own).
    pub fn of(ty: usize) -> Kind {
        Kind::check(ty).unwrap_or(Kind::Off)
    }

    /// Kinds that share state layouts keep their state across a type change.
    fn family(self) -> u8 {
        match self {
            Kind::Off => 0,
            Kind::SvfLp | Kind::SvfHp | Kind::SvfBp | Kind::SvfNotch => 1,
            Kind::Ladder => 2,
            Kind::RbjHp | Kind::RbjBp => 3,
            Kind::Notch => 4,
            #[cfg(feature = "us-flt-v2")]
            Kind::V2(m) if m >= crate::v2::fmode::MOOG_L => 6,
            #[cfg(feature = "us-flt-v2")]
            Kind::V2(_) => 5,
        }
    }
}

/// The highest cutoff a filter slot reaches, Hz (`voice::cut` clamps there).
pub const OPEN_LP_HZ: f32 = 19_980.0;
/// The lowest cutoff the coefficients use, Hz (`coefs` clamps there).
pub const OPEN_HP_HZ: f32 = 1.0;

/// True when the filter is set so that the signal comes out (nearly) as it went in, to what the controls can tell: a
/// low-pass or ladder with the cutoff at the top of its range, or a high-pass with it at the bottom, and no resonance.
/// Such a slot is not run (see [`Slot::pass`]). The test is the end of the range, not a gain of exactly 1, so the sound
/// changes a little: the state-variable low-pass there is 0.4 dB down at 5 kHz, 1.6 dB at 10 kHz and 4 dB at 16 kHz
/// (44.1 kHz; its 2× oversampling rolls the top off whatever the cutoff), the ladder 1.15 dB at full scale (its soft
/// clip), the state-variable high-pass the same 4 dB at 16 kHz and the RBJ high-pass is flat within 0.1 dB. A filter that
/// is bypassed is therefore brighter than the same filter run (Eladamri: +1.9 dB in the 16 kHz octave); the
/// tests `an_open_filter_...` pin these figures.
pub fn transparent(kind: Kind, hz: f32, res: f32) -> bool {
    res <= 0.0
        && match kind {
            Kind::SvfLp if cfg!(feature = "us-flt-svf") => hz >= OPEN_LP_HZ,
            Kind::Ladder if cfg!(feature = "us-flt-ladder") => hz >= OPEN_LP_HZ,
            Kind::SvfHp if cfg!(feature = "us-flt-svf") => hz <= OPEN_HP_HZ,
            Kind::RbjHp if cfg!(feature = "us-flt-rbj") => hz <= OPEN_HP_HZ,
            _ => false,
        }
}

/// The coefficients of `kind` at `hz` and normalized resonance `res`, for `rate`.
pub fn coefs(kind: Kind, hz: f32, res: f32, rate: f32) -> [f32; NC] {
    let hz = hz.max(1.0).min(0.49 * rate);
    let q = res.max(0.0).min(0.85);
    match kind {
        Kind::Off => [0.0; NC],
        // 2× oversampled: f = 1.5·sin(π·fc/2/rate)
        Kind::SvfLp | Kind::SvfHp | Kind::SvfBp | Kind::SvfNotch if cfg!(feature = "us-flt-svf") => {
            [1.5 * crate::fm::sinf(core::f32::consts::PI * hz / (2.0 * rate)), 1.0 - res.max(0.0).min(1.0), 0.0, 0.0, 0.0]
        }
        Kind::Ladder if cfg!(feature = "us-flt-ladder") => {
            let f = 2.0 * hz / rate;
            let k = 3.6 * f - 1.6 * f * f - 1.0;
            let p = (k + 1.0) * 0.5;
            [k, p, q * crate::fm::expf((1.0 - p) * 1.386249), 0.0, 0.0]
        }
        Kind::RbjHp | Kind::RbjBp if cfg!(feature = "us-flt-rbj") => {
            let w0 = 2.0 * core::f32::consts::PI * hz / rate;
            let (sin, cos) = crate::fm::sincosf(w0);
            // the reference's log₁₀2 (not ln 2) bandwidth law
            let e = crate::fm::expf(crate::fm::log10f(2.0) / 2.0 * (1.0 - q) * w0 / sin);
            let alpha = sin * (0.5 * (e - 1.0 / e));
            let a0 = 1.0 + alpha;
            if kind == Kind::RbjHp {
                let h = (1.0 + cos) / 2.0;
                [h / a0, -(1.0 + cos) / a0, h / a0, -2.0 * cos / a0, (1.0 - alpha) / a0]
            } else {
                [sin / 2.0 / a0, 0.0, -(sin / 2.0) / a0, -2.0 * cos / a0, (1.0 - alpha) / a0]
            }
        }
        Kind::Notch if cfg!(feature = "us-flt-notch") => {
            let z = crate::fm::cosf(2.0 * core::f32::consts::PI * hz / rate);
            let b0 = (1.0 - q) * (1.0 - q) / (2.0 * (libm::fabsf(z) + 1.0)) + q;
            [b0, -2.0 * z * b0, b0, -2.0 * z * q, q * q]
        }
        // a kind of a module this build lacks never reaches here (`Kind::of` bypasses it); the guards let the compiler drop the code
        _ => [0.0; NC],
    }
}

/// The coefficients of V2 filter `kind` (`Kind::V2`) from cutoff and resonance (V2 raw / 128) at `rate`. Format
/// versions below 6 have no Moog filter (`old_moog`): those modes then pass the signal (v2redux `DELTA_NO_MOOG`).
#[cfg(feature = "us-flt-v2")]
pub fn coefs_v2(kind: Kind, cutoff: f32, reso: f32, rate: f32, old_moog: bool) -> (Kind, [f32; NC]) {
    match kind {
        Kind::V2(m) => {
            if old_moog && m >= crate::v2::fmode::MOOG_L {
                return (Kind::Off, [0.0; NC]);
            }
            let c = crate::v2::flt_coefs(m, cutoff * 128.0, reso * 128.0, rate);
            (kind, [c[0], c[1], c[2], 0.0, 0.0])
        }
        _ => (Kind::Off, [0.0; NC]),
    }
}

/// One filter slot of a voice (or of the noise band): stereo state and the ramped coefficients.
#[derive(Clone, Copy, Debug, Default)]
pub struct Slot {
    pub kind: Kind,
    c: [Ramp; NC],
    s: [[f32; NS]; 2],
    /// Passed through without running (see [`transparent`]); the coefficients and the state are left alone meanwhile.
    off: bool,
    /// The slot was passed through and is running again: the next control segment primes its state from `last`.
    wake: bool,
    /// The last frame that went past while it was off, to start the state from.
    last: (f32, f32),
}

impl Slot {
    pub const fn new() -> Self {
        Slot { kind: Kind::Off, c: [Ramp::new(0.0); NC], s: [[0.0; NS]; 2], off: false, wake: false, last: (0.0, 0.0) }
    }

    /// Switches to `kind`; a different state layout starts from silence.
    fn switch(&mut self, kind: Kind) -> bool {
        if kind == self.kind {
            return false;
        }
        if kind.family() != self.kind.family() {
            self.s = [[0.0; NS]; 2];
        }
        self.kind = kind;
        true
    }

    /// Says whether the filter does nothing at the coming control segment ([`transparent`]): then it is not run and not
    /// updated. When it stops being transparent its state starts from the signal that went past, as if the filter had
    /// been running on it, so the change does not click. Call before [`Self::set`], [`Self::to`] or [`Self::retarget`].
    pub fn pass(&mut self, on: bool) {
        self.wake |= self.off && !on;
        self.off = on;
    }

    /// Starts running again at `kind` and `c`: the state is the steady state of a signal at the last passed-by level
    /// (a low-pass or ladder holds it, a high-pass, band-pass or notch has none of it at rest).
    fn waking(&mut self, kind: Kind, c: &[f32; NC]) {
        self.wake = false;
        self.kind = kind;
        self.c.iter_mut().zip(c).for_each(|(r, v)| r.set(*v));
        for (s, x) in self.s.iter_mut().zip([self.last.0, self.last.1]) {
            *s = [0.0; NS];
            match kind {
                Kind::SvfLp => (s[0], s[2]) = (x, x),
                Kind::SvfHp | Kind::SvfBp | Kind::SvfNotch => s[2] = x,
                Kind::Ladder => s.iter_mut().for_each(|v| *v = x),
                Kind::RbjHp | Kind::RbjBp | Kind::Notch => s.iter_mut().for_each(|v| *v = x),
                #[cfg(feature = "us-flt-v2")]
                Kind::V2(_) => {}
                Kind::Off => {}
            }
        }
    }

    /// Jumps to these coefficients (voice start).
    pub fn set(&mut self, kind: Kind, c: &[f32; NC]) {
        if self.off {
            self.kind = kind;
            return;
        }
        if self.wake {
            return self.waking(kind, c);
        }
        self.switch(kind);
        self.c.iter_mut().zip(c).for_each(|(r, v)| r.set(*v));
    }

    /// A new control segment of `frames` towards `c`.
    pub fn to(&mut self, kind: Kind, c: &[f32; NC], frames: u32) {
        if self.off {
            self.kind = kind;
            return;
        }
        if self.wake {
            return self.waking(kind, c);
        }
        if self.switch(kind) {
            self.c.iter_mut().zip(c).for_each(|(r, v)| r.set(*v));
        }
        self.c.iter_mut().zip(c).for_each(|(r, v)| r.to(*v, frames));
    }

    /// The rest of a split segment towards `c`.
    pub fn retarget(&mut self, kind: Kind, c: &[f32; NC], frames: u32) {
        if self.off {
            self.kind = kind;
            return;
        }
        if self.wake {
            return self.waking(kind, c);
        }
        if self.switch(kind) {
            self.c.iter_mut().zip(c).for_each(|(r, v)| r.set(*v));
        }
        self.c.iter_mut().zip(c).for_each(|(r, v)| r.retarget(*v, frames));
    }

    /// One stereo frame.
    #[inline]
    pub fn process(&mut self, l: f32, r: f32) -> (f32, f32) {
        if self.off {
            self.last = (l, r);
            return (l, r);
        }
        if self.kind == Kind::Off {
            return (l, r);
        }
        let c = [self.c[0].next(), self.c[1].next(), self.c[2].next(), self.c[3].next(), self.c[4].next()];
        (run(self.kind, &c, &mut self.s[0], l), run(self.kind, &c, &mut self.s[1], r))
    }
}

#[inline]
fn run(kind: Kind, c: &[f32; NC], s: &mut [f32; NS], x: f32) -> f32 {
    match kind {
        Kind::SvfLp | Kind::SvfHp | Kind::SvfBp | Kind::SvfNotch if cfg!(feature = "us-flt-svf") => {
            let (f, q) = (c[0], c[1]);
            let (mut low, mut band) = (s[0], s[1]);
            let mut step = |x: f32| {
                low += f * band;
                let high = q * (x - band) - low;
                band += f * high;
                match kind {
                    Kind::SvfLp => low,
                    Kind::SvfHp => high,
                    Kind::SvfBp => band,
                    _ => low + high,
                }
            };
            let y = (step((s[2] + x) / 2.0) + step(x)) / 2.0;
            s[0] = low;
            s[1] = band;
            s[2] = x;
            y
        }
        Kind::Ladder if cfg!(feature = "us-flt-ladder") => {
            let (k, p, res) = (c[0], c[1], c[2]);
            let xin = x - res * s[3];
            let mut inp = xin;
            for i in 0..4 {
                let y = (s[4 + i] * p + inp * p) - k * s[i];
                s[4 + i] = inp;
                s[i] = y;
                inp = y;
            }
            let y3 = s[3];
            y3 - y3 * y3 * y3 * (1.0 / 6.0)
        }
        Kind::RbjHp | Kind::RbjBp if cfg!(feature = "us-flt-rbj") => biquad(c, s, x),
        Kind::Notch if cfg!(feature = "us-flt-notch") => {
            let y = biquad(c, s, s[4]);
            s[4] = x;
            y
        }
        #[cfg(feature = "us-flt-v2")]
        Kind::V2(m) => {
            use crate::v2::{fmode, Flt, Lrc, Moog};
            let mut f = Flt { lrc: Lrc { l: s[0], b: s[1] }, moog: Moog { b: [s[0], s[1], s[2], s[3], s[4]] } };
            let y = f.run(m, &[c[0], c[1], c[2]], x);
            if m >= fmode::MOOG_L {
                s[..5].copy_from_slice(&f.moog.b);
            } else {
                (s[0], s[1]) = (f.lrc.l, f.lrc.b);
            }
            y
        }
        _ => x,
    }
}

/// Direct form I with state `x1 x2 y1 y2` in `s[0..4]`.
#[inline]
fn biquad(c: &[f32; NC], s: &mut [f32; NS], x: f32) -> f32 {
    let y = c[0] * x + c[1] * s[0] + c[2] * s[1] - c[3] * s[2] - c[4] * s[3];
    s[1] = s[0];
    s[0] = x;
    s[3] = s[2];
    s[2] = y;
    y
}

#[cfg(test)]
// helpers and imports of the tests that need a module this feature set lacks go unused
#[allow(dead_code, unused_imports)]
mod tests {
    use super::*;
    #[cfg(feature = "us-env-timed")]
    use crate::chunk::encode;
    #[cfg(feature = "us-env-timed")]
    use crate::law::host as inv;
    #[cfg(feature = "us-env-timed")]
    use crate::params::{self as pm, f as pf, filt};

    const RATE: f32 = 44100.0;

    /// Deterministic white-ish input.
    fn noise(n: usize) -> std::vec::Vec<f32> {
        let mut s = 0x1234_5678u32;
        (0..n)
            .map(|_| {
                s ^= s << 13;
                s ^= s >> 17;
                s ^= s << 5;
                (s >> 8) as f32 / 8_388_608.0 - 1.0
            })
            .collect()
    }

    fn run(kind: Kind, hz: f32, res: f32, rate: f32, x: &[f32]) -> std::vec::Vec<f32> {
        let mut f = Slot::new();
        f.set(kind, &coefs(kind, hz, res, rate));
        x.iter().map(|v| f.process(*v, *v).0).collect()
    }

    /// Steady-state gain of a filter for a sine at `hz`.
    fn gain(kind: Kind, cutoff: f32, res: f32, rate: f32, hz: f32) -> f32 {
        let n = (rate as usize) / 2;
        let x: std::vec::Vec<f32> = (0..n).map(|i| libm::sinf(2.0 * core::f32::consts::PI * hz * i as f32 / rate)).collect();
        let y = run(kind, cutoff, res, rate, &x);
        let rms = |v: &[f32]| (v.iter().map(|a| (*a as f64).powi(2)).sum::<f64>() / v.len() as f64).sqrt();
        (rms(&y[n / 2..]) / rms(&x[n / 2..])) as f32
    }

    fn db(g: f32) -> f32 {
        20.0 * libm::log10f(g.max(1e-9))
    }

    fn worst(a: &[f32], b: &[f32]) -> f32 {
        a.iter().zip(b).fold(0.0f32, |m, (x, y)| m.max((x - y).abs()))
    }

    /// WaveSabre's state-variable filter in f64, written out independently.
    fn svf_reference(kind: Kind, hz: f64, res: f64, rate: f64, x: &[f32]) -> std::vec::Vec<f64> {
        let f = 1.5 * (core::f64::consts::PI * hz / 2.0 / rate).sin();
        let q = 1.0 - res;
        let (mut low, mut band, mut last) = (0.0f64, 0.0f64, 0.0f64);
        let mut step = |x: f64| {
            low += f * band;
            let high = q * (x - band) - low;
            band += f * high;
            match kind {
                Kind::SvfLp => low,
                Kind::SvfHp => high,
                Kind::SvfBp => band,
                _ => low + high,
            }
        };
        x.iter()
            .map(|v| {
                let v = *v as f64;
                let y = (step((last + v) / 2.0) + step(v)) / 2.0;
                last = v;
                y
            })
            .collect()
    }

    #[test]
    #[cfg(all(feature = "us-flt-svf", feature = "us-flt-ladder", feature = "us-flt-rbj", feature = "us-flt-notch"))]
    fn the_state_variable_filter_is_the_chamberlin_filter() {
        let x = noise(8000);
        for kind in [Kind::SvfLp, Kind::SvfHp, Kind::SvfBp, Kind::SvfNotch] {
            for (hz, res) in [(300.0f32, 0.0f32), (2000.0, 0.5), (9000.0, 0.9)] {
                let got = run(kind, hz, res, RATE, &x);
                let want = svf_reference(kind, hz as f64, res as f64, RATE as f64, &x);
                let err = got.iter().zip(&want).fold(0.0f64, |m, (a, b)| m.max((*a as f64 - b).abs()));
                assert!(err < 2e-3, "{kind:?} {hz} {res}: {err}");
            }
        }
    }

    #[test]
    #[cfg(all(feature = "us-flt-svf", feature = "us-flt-ladder", feature = "us-flt-rbj", feature = "us-flt-notch"))]
    fn resonance_follows_each_types_original_law() {
        // SVF: q = 1 − p
        assert!((coefs(Kind::SvfLp, 1000.0, 0.3, RATE)[1] - 0.7).abs() < 1e-6);
        assert_eq!(coefs(Kind::SvfLp, 1000.0, 0.0, RATE)[1], 1.0);
        assert_eq!(coefs(Kind::SvfLp, 1000.0, 1.0, RATE)[1], 0.0);
        // the others clamp to 0..=0.85: 0.9 is 0.85
        for kind in [Kind::Ladder, Kind::RbjHp, Kind::RbjBp, Kind::Notch] {
            assert_eq!(coefs(kind, 1000.0, 0.9, RATE), coefs(kind, 1000.0, 0.85, RATE), "{kind:?}");
            assert_ne!(coefs(kind, 1000.0, 0.5, RATE), coefs(kind, 1000.0, 0.1, RATE), "{kind:?}");
        }
        // ladder feedback r = q·e^((1 − p)·1.386249) with p from the cutoff
        let c = coefs(Kind::Ladder, 2205.0, 0.5, RATE);
        let f = 2.0 * 2205.0 / RATE;
        let p = (3.6 * f - 1.6 * f * f) / 2.0;
        assert!((c[2] - 0.5 * libm::expf((1.0 - p) * 1.386249)).abs() < 1e-5);
    }

    #[test]
    #[cfg(all(feature = "us-flt-svf", feature = "us-flt-ladder", feature = "us-flt-rbj", feature = "us-flt-notch"))]
    fn each_type_has_its_response_one_octave_either_side_of_the_cutoff() {
        let (lo, hi) = (500.0, 2000.0);
        for rate in [44100.0f32, 48000.0, 96000.0] {
            // WaveSabre's coefficient is 1.5·sin(…) where the Chamberlin filter wants 2·sin(…): the SVF's real corner is 3/4
            // of the cutoff parameter, which the port keeps, so those tests aim the parameter at 4/3 of the corner
            let g = |k: Kind, q: f32, hz: f32| {
                let svf = matches!(k, Kind::SvfLp | Kind::SvfHp | Kind::SvfBp | Kind::SvfNotch);
                db(gain(k, if svf { 1000.0 / 0.75 } else { 1000.0 }, q, rate, hz))
            };
            // low-passes: pass an octave below, at least 9 dB down an octave above
            for k in [Kind::SvfLp, Kind::Ladder] {
                assert!(g(k, 0.0, lo) > -4.0 && g(k, 0.0, hi) < g(k, 0.0, lo) - 9.0, "{k:?} @{rate}: {} {}", g(k, 0.0, lo), g(k, 0.0, hi));
            }
            assert!(g(Kind::Ladder, 0.0, 4000.0) < -20.0, "four poles");
            // high-passes
            for k in [Kind::SvfHp, Kind::RbjHp] {
                assert!(g(k, 0.0, hi) > -4.0 && g(k, 0.0, lo) < g(k, 0.0, hi) - 5.0, "{k:?} @{rate}: {} {}", g(k, 0.0, lo), g(k, 0.0, hi));
            }
            // band-passes peak around the cutoff
            for k in [Kind::SvfBp, Kind::RbjBp] {
                let mid = g(k, 0.7, 1000.0);
                assert!(mid > g(k, 0.7, lo) + 2.0 && mid > g(k, 0.7, hi) + 2.0, "{k:?} @{rate}: {mid} {} {}", g(k, 0.7, lo), g(k, 0.7, hi));
            }
            // notches reject the cutoff and pass the octaves
            for k in [Kind::SvfNotch, Kind::Notch] {
                let mid = g(k, 0.5, 1000.0);
                // the SVF scales its input by q = 0.5 (−6 dB), which the edges inherit
                let floor = if k == Kind::SvfNotch { -8.0 } else { -50.0 };
                assert!(mid < -12.0 && g(k, 0.5, lo) > floor && g(k, 0.5, hi) > floor && g(k, 0.5, lo) > mid + 12.0 && g(k, 0.5, hi) > mid + 12.0, "{k:?} @{rate}: {mid} {} {}", g(k, 0.5, lo), g(k, 0.5, hi));
            }
        }
    }

    #[test]
    #[cfg(all(feature = "us-flt-svf", feature = "us-flt-ladder", feature = "us-flt-rbj", feature = "us-flt-notch"))]
    fn resonance_raises_the_peak_at_the_cutoff() {
        // the SVF's input is scaled by q = 1 − p, so resonance is judged against the passband: the peak at the cutoff
        // over the level well below it
        let at = |q: f32| gain(Kind::SvfLp, 1000.0, q, RATE, 1000.0) / gain(Kind::SvfLp, 1000.0, q, RATE, 100.0);
        assert!(at(0.9) > 1.5 * at(0.0), "{} {}", at(0.9), at(0.0));
        let ladder = |q: f32| (6..=12).map(|k| gain(Kind::Ladder, 1000.0, q, RATE, 100.0 * k as f32)).fold(0.0f32, f32::max);
        assert!(ladder(0.85) > ladder(0.0) * 1.5, "{} {}", ladder(0.85), ladder(0.0));
    }

    #[test]
    fn a_type_that_shares_a_state_layout_keeps_its_state_and_another_starts_clean() {
        let x = noise(500);
        let mut f = Slot::new();
        f.set(Kind::SvfLp, &coefs(Kind::SvfLp, 800.0, 0.2, RATE));
        x.iter().for_each(|v| _ = f.process(*v, *v));
        let before = f.s;
        f.to(Kind::SvfHp, &coefs(Kind::SvfHp, 800.0, 0.2, RATE), 32);
        assert_eq!(f.s, before, "SVF kinds share their state");
        f.to(Kind::Ladder, &coefs(Kind::Ladder, 800.0, 0.2, RATE), 32);
        assert_eq!(f.s, [[0.0; NS]; 2], "a different layout starts from silence");
        f.to(Kind::Off, &[0.0; NC], 32);
        assert_eq!(f.process(0.25, -0.5), (0.25, -0.5), "off is bypass");
    }

    #[test]
    #[cfg(all(feature = "us-flt-svf", feature = "us-flt-ladder", feature = "us-flt-rbj", feature = "us-flt-notch"))]
    fn coefficients_ramp_across_the_segment() {
        let mut f = Slot::new();
        let (a, b) = (coefs(Kind::SvfLp, 200.0, 0.0, RATE), coefs(Kind::SvfLp, 4000.0, 0.0, RATE));
        f.set(Kind::SvfLp, &a);
        f.to(Kind::SvfLp, &b, 32);
        let mut seen = std::vec::Vec::new();
        for _ in 0..33 {
            seen.push(f.c[0].next());
        }
        assert_eq!(seen[0], a[0]);
        assert!((seen[32] - b[0]).abs() < 1e-5 && (seen[16] - (a[0] + b[0]) / 2.0).abs() < 1e-5);
        assert!(seen.windows(2).all(|w| w[1] > w[0]));
    }

    #[cfg(feature = "us-env-timed")] // the default patch
    #[test]
    fn a_missing_filter_module_is_named_and_a_present_one_loads() {
        let patch = |ty: usize| {
            let mut v = pm::DEFAULTS;
            v[filt(3, pf::TYPE)] = inv::inv_choice(ty, pm::filter_type::COUNT);
            encode(&v, &[])
        };
        for (ty, on, id) in [
            (ft::SVF_LP, cfg!(feature = "us-flt-svf"), module::FILTER_SVF),
            (ft::SVF_NOTCH, cfg!(feature = "us-flt-svf"), module::FILTER_SVF),
            (ft::LADDER_LP, cfg!(feature = "us-flt-ladder"), module::FILTER_LADDER),
            (ft::RBJ_HP, cfg!(feature = "us-flt-rbj"), module::FILTER_RBJ),
            (ft::RBJ_BP, cfg!(feature = "us-flt-rbj"), module::FILTER_RBJ),
            (ft::NOTCH, cfg!(feature = "us-flt-notch"), module::FILTER_NOTCH),
        ] {
            let r = crate::instrument::UInstrument::need(&patch(ty));
            if on {
                assert!(r.is_ok(), "type {ty}");
                assert_eq!(Kind::check(ty).is_ok(), true);
            } else {
                assert_eq!(r.err(), Some(LoadError::UnsupportedModule(id)), "type {ty}");
                assert_eq!(Kind::of(ty), Kind::Off, "automated to a missing type = bypass");
            }
        }
        assert!(crate::instrument::UInstrument::need(&patch(ft::OFF)).is_ok());
    }

    #[test]
    fn the_cutoff_is_kept_below_nyquist() {
        for k in [Kind::SvfLp, Kind::Ladder, Kind::RbjHp, Kind::RbjBp, Kind::Notch] {
            assert_eq!(coefs(k, 1.0e6, 0.3, 22050.0), coefs(k, 0.49 * 22050.0, 0.3, 22050.0), "{k:?}");
            assert_eq!(coefs(k, -5.0, 0.3, 22050.0), coefs(k, 1.0, 0.3, 22050.0), "{k:?}: a negative cutoff");
        }
        for k in [Kind::SvfLp, Kind::Ladder, Kind::RbjHp, Kind::Notch] {
            for c in coefs(k, 100_000.0, 0.5, 22050.0) {
                assert!(c.is_finite(), "{k:?}");
            }
        }
        let y = run(Kind::Ladder, 19_980.0, 0.85, 22050.0, &noise(20_000));
        // the ladder at full cutoff and resonance is wild at any rate (about 10 on unit noise at 44.1 kHz, like Tunefish's)
        let peak = |v: &[f32]| v.iter().fold(0.0f32, |m, a| m.max(a.abs()));
        let y44 = run(Kind::Ladder, 19_980.0, 0.85, 44100.0, &noise(20_000));
        assert!(y.iter().all(|v| v.is_finite()) && peak(&y) < 2.0 * peak(&y44), "{} {}", peak(&y), peak(&y44));
    }

    /// Steady-state gain in dB of `kind` at `hz` (cutoff, no resonance) for a sine at `f`.
    fn open_gain_db(kind: Kind, hz: f32, f: f32, rate: f32, amp: f32) -> f32 {
        let c = coefs(kind, hz, 0.0, rate);
        let mut s = [0.0f32; NS];
        let n = rate as usize / 2;
        let (mut e_out, mut e_in) = (0.0f64, 0.0f64);
        for i in 0..n {
            let x = (amp as f64 * libm::sin(2.0 * core::f64::consts::PI * f as f64 * i as f64 / rate as f64)) as f32;
            let y = super::run(kind, &c, &mut s, x);
            if i > n / 2 {
                e_out += (y * y) as f64;
                e_in += (x * x) as f64;
            }
        }
        (10.0 * (e_out / e_in).log10()) as f32
    }

    #[test]
    fn a_filter_is_transparent_only_at_the_open_end_of_its_range_without_resonance() {
        let cases = [
            (Kind::SvfLp, OPEN_LP_HZ, 0.0, true, "svf"),
            (Kind::Ladder, OPEN_LP_HZ, 0.0, true, "ladder"),
            (Kind::SvfHp, OPEN_HP_HZ, 0.0, true, "svf hp"),
            (Kind::RbjHp, OPEN_HP_HZ, 0.0, true, "rbj hp"),
            (Kind::SvfLp, OPEN_LP_HZ - 1.0, 0.0, false, "a hertz short of the top"),
            (Kind::SvfLp, 10_000.0, 0.0, false, "mid range"),
            (Kind::SvfLp, OPEN_LP_HZ, 0.1, false, "resonance"),
            (Kind::SvfHp, OPEN_HP_HZ + 0.5, 0.0, false, "a high-pass just above the bottom"),
            (Kind::SvfHp, OPEN_LP_HZ, 0.0, false, "a high-pass at the top"),
            (Kind::SvfLp, OPEN_HP_HZ, 0.0, false, "a low-pass at the bottom"),
        ];
        for (k, hz, res, want, what) in cases {
            let have = |k: Kind| match k {
                Kind::SvfLp | Kind::SvfHp => cfg!(feature = "us-flt-svf"),
                Kind::Ladder => cfg!(feature = "us-flt-ladder"),
                _ => cfg!(feature = "us-flt-rbj"),
            };
            assert_eq!(transparent(k, hz, res), want && have(k), "{what}");
        }
        // a band-pass, a notch and an off slot are never passed through, at any setting
        for k in [Kind::SvfBp, Kind::SvfNotch, Kind::RbjBp, Kind::Notch, Kind::Off] {
            for hz in [OPEN_HP_HZ, 1000.0, OPEN_LP_HZ] {
                assert!(!transparent(k, hz, 0.0), "{k:?} {hz}");
            }
        }
    }

    #[cfg(all(feature = "us-flt-svf", feature = "us-flt-ladder", feature = "us-flt-rbj"))]
    #[test]
    fn an_open_filter_changes_the_sound_by_no_more_than_the_documented_amounts() {
        for rate in [44100.0f32, 48000.0] {
            // (kind, cutoff, [(frequency, allowed loss dB)]): measured 0.02 / 0.4 / 1.63 / 4.02 dB (high-pass 4.19) at 44.1 kHz
            let lp = [(1000.0, 0.05), (5000.0, 0.5), (10_000.0, 1.7), (16_000.0, 4.1)];
            for (k, hz, probes) in [
                (Kind::SvfLp, OPEN_LP_HZ, &lp[..]),
                (Kind::SvfHp, OPEN_HP_HZ, &[(1000.0, 0.05), (5000.0, 0.5), (10_000.0, 1.7), (16_000.0, 4.3)][..]),
                (Kind::RbjHp, OPEN_HP_HZ, &[(20.0, 0.1), (1000.0, 0.05), (16_000.0, 0.1)][..]),
            ] {
                for &(f, loss) in probes {
                    let g = open_gain_db(k, hz, f, rate, 0.1);
                    assert!(g <= 0.1 && g >= -loss, "{k:?} at {f} Hz, {rate}: {g} dB (allowed -{loss})");
                }
            }
            // the ladder is flat at small signals; at full scale its soft clip costs 1.15 dB
            assert!(open_gain_db(Kind::Ladder, OPEN_LP_HZ, 1000.0, rate, 0.05).abs() < 0.02);
            let full = open_gain_db(Kind::Ladder, OPEN_LP_HZ, 1000.0, rate, 1.0);
            assert!((-1.3..=-1.0).contains(&full), "{full}");
        }
    }

    #[test]
    fn a_passed_through_slot_neither_runs_nor_changes() {
        let c = coefs(Kind::SvfLp, OPEN_LP_HZ, 0.0, RATE);
        let mut f = Slot::new();
        f.set(Kind::SvfLp, &c);
        for i in 0..200 {
            f.process(0.3 * libm::sinf(i as f32 * 0.1), 0.0);
        }
        let (state, coefs_now) = (f.s, std::format!("{:?}", f.c));
        f.pass(true);
        f.to(Kind::SvfLp, &c, 32);
        for i in 0..500 {
            let x = 0.5 * libm::sinf(i as f32 * 0.05);
            assert_eq!(f.process(x, -x), (x, -x), "the input goes out unchanged");
        }
        assert!(f.s == state && std::format!("{:?}", f.c) == coefs_now, "not run, not updated");
        assert_eq!(f.last, (0.5 * libm::sinf(499.0 * 0.05), -0.5 * libm::sinf(499.0 * 0.05)));
        // an Off slot passes too, without keeping the last frame
        let mut off = Slot::new();
        assert_eq!(off.process(1.0, 2.0), (1.0, 2.0));
        assert_eq!(off.last, (0.0, 0.0));
    }

    /// The worst jump between a filter that ran all along and one that was passed through and woke at `from`, in the
    /// first 200 frames after, for a 440 Hz sine of amplitude 0.5.
    fn wake_gap(kind: Kind, open_hz: f32, hz_closed: f32, prime: bool) -> f32 {
        let closed = kind;
        let x = |i: usize| 0.5 * libm::sinf(2.0 * core::f32::consts::PI * 440.0 * i as f32 / RATE);
        let (open, shut) = (coefs(kind, open_hz, 0.0, RATE), coefs(closed, hz_closed, 0.0, RATE));
        let mut run_all = Slot::new();
        run_all.set(kind, &open);
        let mut woken = Slot::new();
        woken.set(kind, &open);
        woken.pass(true);
        woken.to(kind, &open, 32);
        let from = 4000;
        for i in 0..from {
            let (a, _) = run_all.process(x(i), x(i));
            let (b, _) = woken.process(x(i), x(i));
            let _ = (a, b);
        }
        // both continue at the same new cutoff; the woken one starts from the primed (or zeroed) state
        run_all.set(closed, &shut);
        woken.pass(false);
        woken.set(closed, &shut);
        if !prime {
            woken.last = (0.0, 0.0);
            woken.s = [[0.0; NS]; 2];
        }
        let mut worst = 0.0f32;
        for i in from..from + 200 {
            let (a, _) = run_all.process(x(i), x(i));
            let (b, _) = woken.process(x(i), x(i));
            worst = worst.max((a - b).abs());
        }
        worst
    }

    #[cfg(all(feature = "us-flt-svf", feature = "us-flt-ladder", feature = "us-flt-rbj"))]
    #[test]
    fn a_filter_that_wakes_starts_from_the_signal_that_went_past() {
        // low-pass open → low-pass at 5 kHz: close to the filter that ran all along; from silence it would drop out first
        let primed = wake_gap(Kind::SvfLp, OPEN_LP_HZ, 5000.0, true);
        assert!(primed < 0.03, "svf low-pass primed: {primed}");
        let ladder = wake_gap(Kind::Ladder, OPEN_LP_HZ, 5000.0, true);
        assert!(ladder < 0.03, "ladder primed: {ladder}");
        // high-pass at the bottom → high-pass at 200 Hz
        let hp = wake_gap(Kind::RbjHp, OPEN_HP_HZ, 200.0, true);
        assert!(hp < 0.03, "rbj high-pass primed: {hp}");
        // the priming is what does it: from a zero state the gap is several times larger
        let cold = wake_gap(Kind::SvfLp, OPEN_LP_HZ, 5000.0, false);
        assert!(cold > 5.0 * primed.max(0.01), "cold {cold} against primed {primed}");
    }
}
