//! Filter slots (design D4): the state-variable filter (one Chamberlin step, 2× oversampled, in two forms: see
//! [`SvForm`]) with low/band/high/notch/all-pass responses, the 4-pole ladder low-pass, the 2× 4-pole filter, RBJ
//! high/band-pass and the resonator notch. Coefficients are computed for the song rate at control rate, from the cutoff
//! in Hz (the unified law, applied by the caller) or from 7-bit steps ([`coefs_steps`]); a segment's coefficients ramp
//! linearly from the previous segment's, so a cutoff sweep has no steps. Resonance keeps each type's original law:
//! state-variable `1 − p`; the others clamp `p` to `0..=0.85`.

use crate::module;
use crate::params::filter_type as ft;
use crate::ramp::Ramp;
use synth_core::mix::LoadError;

/// Coefficients per filter (the biquads use all five).
pub const NC: usize = 5;
/// State floats per channel.
pub const NS: usize = 9;

/// The feedback and 2× forms and the 7-bit step law are built with a module that uses them (the
/// feedback-form filter types, the drive, the channel strip's drive); without one, the state-variable and 4-pole steps
/// compile to the plain input-form and one-step arithmetic.
const FORMS: bool = cfg!(any(feature = "us-flt-feedback", feature = "us-drive", feature = "us-strip"));

/// The kinds of the 7-bit step-law types in order (`filter_type` from `V2_LP`, the drive's filter modes from 5).
pub const STEPS_KINDS: [Kind; 7] = [Kind::SvfLp, Kind::SvfBp, Kind::SvfHp, Kind::SvfNotch, Kind::SvfAll, Kind::Pole4Lp, Kind::Pole4Hp];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Kind {
    #[default]
    Off,
    /// State-variable responses (the form is in the coefficients, [`sv_coefs`]).
    SvfLp,
    SvfHp,
    SvfBp,
    SvfNotch,
    /// Low + band + high: flat at zero resonance, a band-pass peak above it.
    SvfAll,
    /// 4-pole responses: the low-pass, and the high-pass as the input minus it (the form is in the coefficients,
    /// [`P4Form`]).
    Pole4Lp,
    Pole4Hp,
    RbjHp,
    RbjBp,
    Notch,
}

impl Kind {
    /// The type for a `filter_type` value, or the module that is missing from this build.
    pub fn check(ty: usize) -> Result<Kind, LoadError> {
        let (kind, id, on) = match ty {
            ft::SVF_LP => (Kind::SvfLp, module::FILTER_SVF, cfg!(feature = "us-flt-svf")),
            ft::SVF_HP => (Kind::SvfHp, module::FILTER_SVF, cfg!(feature = "us-flt-svf")),
            ft::SVF_BP => (Kind::SvfBp, module::FILTER_SVF, cfg!(feature = "us-flt-svf")),
            ft::SVF_NOTCH => (Kind::SvfNotch, module::FILTER_SVF, cfg!(feature = "us-flt-svf")),
            ft::LADDER_LP => (Kind::Pole4Lp, module::FILTER_LADDER, cfg!(feature = "us-flt-ladder")),
            ft::RBJ_HP => (Kind::RbjHp, module::FILTER_RBJ, cfg!(feature = "us-flt-rbj")),
            ft::RBJ_BP => (Kind::RbjBp, module::FILTER_RBJ, cfg!(feature = "us-flt-rbj")),
            ft::NOTCH => (Kind::Notch, module::FILTER_NOTCH, cfg!(feature = "us-flt-notch")),
            ft::V2_LP..=ft::V2_MOOG_HP => (STEPS_KINDS[ty - ft::V2_LP], module::FILTER_FEEDBACK, cfg!(feature = "us-flt-feedback")),
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
            Kind::SvfLp | Kind::SvfHp | Kind::SvfBp | Kind::SvfNotch | Kind::SvfAll => 1,
            Kind::Pole4Lp | Kind::Pole4Hp => 2,
            Kind::RbjHp | Kind::RbjBp => 3,
            Kind::Notch => 4,
        }
    }
}

/// True when a `filter_type` value uses the 7-bit step law and the feedback form ([`coefs_steps`]): the later kinds.
/// Until the format switch (design D4, "Order of work") the type value carries the law and the form.
pub fn steps(ty: usize) -> bool {
    ty >= ft::V2_LP
}

/// The state-variable form (design D1). `feedback`: the resonance damps the band feedback only
/// (`high = x − (1 − r)·band − low`) instead of scaling the input too (`high = q·(x − band) − low`). `held`: both
/// oversampling steps take the input and the output is the second step's, instead of the first step taking the mean of
/// the previous and the current input and the output being the mean of both steps. (The source's tiny DC bias against
/// denormals is gone: the player flushes them, and no song moved without it; unify-v2-algorithms 6.4.)
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct SvForm {
    pub feedback: bool,
    pub held: bool,
}

impl SvForm {
    /// Input-scaled resonance, interpolated input, averaged output.
    pub const INPUT: SvForm = SvForm { feedback: false, held: false };
    /// Feedback-scaled resonance, held input, last output.
    pub const FEEDBACK: SvForm = SvForm { feedback: true, held: true };
}

/// State-variable coefficients `[f, a, c, α, 0]` for frequency coefficient `f` and normalized resonance `res`: one
/// step is `low += f·band; high = a·(x − c·band) − low; band += f·high`, α the weight of the interpolated half-step.
pub fn sv_coefs(f: f32, res: f32, form: SvForm) -> [f32; NC] {
    let d = 1.0 - res.max(0.0).min(1.0);
    let (a, c) = if form.feedback { (1.0, d) } else { (d, 1.0) };
    [f, a, c, if form.held { 0.0 } else { 0.5 }, 0.0]
}

/// The state-variable frequency coefficient on the Hz law: `1.5·sin(π·hz/2/rate)` (2× oversampled; the 1.5 puts the
/// real corner at 3/4 of `hz`, as the original does).
pub fn sv_f_hz(hz: f32, rate: f32) -> f32 {
    1.5 * crate::fm::sinf(core::f32::consts::PI * hz / (2.0 * rate))
}

/// The state-variable frequency coefficient on the 7-bit step law (`cutoff` = step / 128).
pub fn sv_f_steps(cutoff: f32, rate: f32) -> f32 {
    crate::v2::calcfreq(cutoff) * crate::v2::linfreq(rate)
}

/// The 4-pole form (design D1). `clip_after`: the feedback and the last pole's state take the clipped output (and the
/// first pole's previous input is the dry input) instead of the unclipped last pole (and the fed-back input). `x2`:
/// two steps per sample on the same input.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct P4Form {
    pub clip_after: bool,
    pub x2: bool,
}

impl P4Form {
    /// Feedback before the clip, one step.
    pub const ONE: P4Form = P4Form { clip_after: false, x2: false };
    /// Feedback after the clip, two steps.
    pub const TWO: P4Form = P4Form { clip_after: true, x2: true };
}

/// 4-pole coefficients `[g, p, fb, form, 0]` for frequency coefficient `f` (from a law: [`p4_f_hz`],
/// [`p4_f_steps`]) and normalized resonance `res`. One step is `xin = x − fb·state₄`, each pole
/// `(in + prev)·p + g·state`, then the cubic clip `y − y³/6`, with `p = f + 0.8·f·(1 − f)`, `g = 1 − 2p` (the ladder's
/// and the 2× filter's poles are the same function of `f`). The feedback follows the clip form, which decides what is
/// stable: before the clip (a linear loop) `min(res, 0.85)·e^{1.386249·(1 − p)}`, after it
/// `4·res·(1 + ½·t·(5.6t² + 1 − t))`, `t = 1 − f`. `form` codes [`P4Form`] as a whole number: 1 = clip after,
/// 2 = two steps.
pub fn p4_coefs(f: f32, res: f32, form: P4Form) -> [f32; NC] {
    let t = 1.0 - f;
    let p = f + 0.8 * (f * t);
    let fb = if form.clip_after {
        let qx = 5.6 * (t * t) + (1.0 - t);
        let qb = res * (1.0 + 0.5 * (t * qx));
        let q2 = qb + qb;
        q2 + q2
    } else {
        res.max(0.0).min(0.85) * crate::fm::expf((1.0 - p) * 1.386249)
    };
    let code = form.clip_after as u32 + 2 * form.x2 as u32;
    [1.0 - (p + p), p, fb, code as f32, 0.0]
}

/// The 4-pole frequency coefficient on the Hz law: `2·hz` over the step rate.
pub fn p4_f_hz(hz: f32, rate: f32, form: P4Form) -> f32 {
    2.0 * hz / (rate * if form.x2 { 2.0 } else { 1.0 })
}

/// The 4-pole frequency coefficient on the 7-bit step law (`cutoff` = step / 128).
pub fn p4_f_steps(cutoff: f32, rate: f32, form: P4Form) -> f32 {
    crate::v2::calcfreq(cutoff) * crate::v2::linfreq(rate) * if form.x2 { 0.25 } else { 0.5 }
}

/// The highest cutoff a filter slot reaches, Hz (`voice::cut` clamps there).
pub const OPEN_LP_HZ: f32 = 19_980.0;
/// The lowest cutoff the coefficients use, Hz (`coefs` clamps there).
pub const OPEN_HP_HZ: f32 = 1.0;

#[cfg(feature = "probe")]
std::thread_local! {
    static RUN_OPEN: core::cell::Cell<bool> = const { core::cell::Cell::new(false) };
}

/// Editor-only listening switch (feature `probe`, never in the player): run open filters instead of skipping them,
/// so the top end rolls off as in the legacy engines (WaveSabre's state-variable low-pass, −4 dB at 16 kHz even fully
/// open). Per thread: the DAW's live host sets it; bounces and exports are unaffected.
#[cfg(feature = "probe")]
pub fn set_run_open(on: bool) {
    RUN_OPEN.with(|c| c.set(on));
}

/// True when the filter is set so that the signal comes out (nearly) as it went in, to what the controls can tell: a
/// low-pass or ladder with the cutoff at the top of its range, or a high-pass with it at the bottom, and no resonance.
/// Such a slot is not run (see [`Slot::pass`]). The test is the end of the range, not a gain of exactly 1, so the sound
/// changes a little: the state-variable low-pass there is 0.4 dB down at 5 kHz, 1.6 dB at 10 kHz and 4 dB at 16 kHz
/// (44.1 kHz; its 2× oversampling rolls the top off whatever the cutoff), the ladder 1.15 dB at full scale (its soft
/// clip), the state-variable high-pass the same 4 dB at 16 kHz and the RBJ high-pass is flat within 0.1 dB. A filter that
/// is bypassed is therefore brighter than the same filter run (Eladamri: +1.9 dB in the 16 kHz octave); the
/// tests `an_open_filter_...` pin these figures.
pub fn transparent(kind: Kind, hz: f32, res: f32) -> bool {
    #[cfg(feature = "probe")]
    if RUN_OPEN.with(|c| c.get()) {
        return false;
    }
    res <= 0.0
        && match kind {
            Kind::SvfLp if cfg!(feature = "us-flt-svf") => hz >= OPEN_LP_HZ,
            Kind::Pole4Lp if cfg!(feature = "us-flt-ladder") => hz >= OPEN_LP_HZ,
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
        Kind::SvfLp | Kind::SvfHp | Kind::SvfBp | Kind::SvfNotch | Kind::SvfAll if cfg!(feature = "us-flt-svf") => {
            sv_coefs(sv_f_hz(hz, rate), res, SvForm::INPUT)
        }
        Kind::Pole4Lp | Kind::Pole4Hp if cfg!(feature = "us-flt-ladder") => p4_coefs(p4_f_hz(hz, rate, P4Form::ONE), res, P4Form::ONE),
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

/// The coefficients of `kind` on the 7-bit step law (cutoff and resonance = step / 128) at `rate`: the state-variable
/// responses in the feedback form, the 2× 4-pole filter. Format versions below 6 have no 4-pole filter (`old_moog`):
/// it then passes the signal (v2redux `DELTA_NO_MOOG`; moves to the converter at the format switch, design D3).
pub fn coefs_steps(kind: Kind, cutoff: f32, reso: f32, rate: f32, old_moog: bool) -> (Kind, [f32; NC]) {
    match kind {
        Kind::Off => (Kind::Off, [0.0; NC]),
        Kind::Pole4Lp | Kind::Pole4Hp => {
            if old_moog {
                return (Kind::Off, [0.0; NC]);
            }
            (kind, p4_coefs(p4_f_steps(cutoff, rate, P4Form::TWO), reso, P4Form::TWO))
        }
        _ => (kind, sv_coefs(sv_f_steps(cutoff, rate), reso, SvForm::FEEDBACK)),
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
                Kind::SvfHp | Kind::SvfBp | Kind::SvfNotch | Kind::SvfAll => s[2] = x,
                Kind::Pole4Lp | Kind::Pole4Hp | Kind::RbjHp | Kind::RbjBp | Kind::Notch => s.iter_mut().for_each(|v| *v = x),
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

/// One sample of `kind` with coefficients `c` on one channel's state `s` (the filter slots, the drive's filter modes).
#[inline]
pub fn run(kind: Kind, c: &[f32; NC], s: &mut [f32; NS], x: f32) -> f32 {
    match kind {
        // one formula for both forms (design D1), exact for each: the input form is `(a, c, α) = (q, 1, ½)`, the feedback
        // form `(1, 1 − r, 0)` (`sv_coefs`)
        Kind::SvfLp | Kind::SvfHp | Kind::SvfBp | Kind::SvfNotch | Kind::SvfAll
            if cfg!(feature = "us-flt-svf") || FORMS =>
        {
            let (f, a) = (c[0], c[1]);
            // without the forms: c = 1, α = ½ as constants (the plain input-form arithmetic)
            let (cb, al) = if FORMS { (c[2], c[3]) } else { (1.0, 0.5) };
            let (mut low, mut band) = (s[0], s[1]);
            let mut step = |x: f32| {
                low += f * band;
                let high = a * (x - cb * band) - low;
                band += f * high;
                match kind {
                    Kind::SvfLp => low,
                    Kind::SvfHp => high,
                    Kind::SvfBp => band,
                    Kind::SvfNotch => low + high,
                    _ => high + band + low,
                }
            };
            let y = if FORMS {
                let y1 = step(al * s[2] + (1.0 - al) * x);
                al * y1 + (1.0 - al) * step(x)
            } else {
                (step((s[2] + x) / 2.0) + step(x)) / 2.0
            };
            s[0] = low;
            s[1] = band;
            s[2] = x;
            y
        }
        // one formula for both forms (design D1): poles in `s[0..4]`, the first pole's previous input in `s[4]`; exact
        // for the two-step form, the one-step form rounds `(in + prev)·p` where the ladder had `prev·p + in·p`
        Kind::Pole4Lp | Kind::Pole4Hp if cfg!(feature = "us-flt-ladder") || FORMS => {
            let (g, p, fb) = (c[0], c[1], c[2]);
            // without the forms: one step, feedback before the clip, as constants
            let code = if FORMS { c[3] as u32 } else { 0 };
            let after = code & 1 != 0;
            let mut y = 0.0;
            for _ in 0..1 + (code >> 1) {
                let xin = x - fb * s[3];
                let (t0, t1, t2) = (s[0], s[1], s[2]);
                s[0] = (xin + s[4]) * p + s[0] * g;
                s[1] = (t0 + s[0]) * p + s[1] * g;
                s[2] = (t1 + s[1]) * p + s[2] * g;
                let y4 = (t2 + s[2]) * p + s[3] * g;
                y = y4 - y4 * y4 * y4 * (1.0 / 6.0);
                (s[3], s[4]) = if after { (y, x) } else { (y4, xin) };
            }
            if kind == Kind::Pole4Hp { x - y } else { y }
        }
        Kind::RbjHp | Kind::RbjBp if cfg!(feature = "us-flt-rbj") => biquad(c, s, x),
        Kind::Notch if cfg!(feature = "us-flt-notch") => {
            let y = biquad(c, s, s[4]);
            s[4] = x;
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
    #[cfg(feature = "us-env-seconds")]
    use crate::chunk::encode;
    #[cfg(feature = "us-env-seconds")]
    use crate::law::host as inv;
    #[cfg(feature = "us-env-seconds")]
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

    /// V2's LRC step, 2× oversampled (`v2::Lrc::step_2x` before change unify-v2-algorithms), without its input bias:
    /// the merged filter dropped it (6.4: no song moved; it leaves a standing DC near −60 dB in low and band).
    struct Lrc2 {
        l: f32,
        b: f32,
    }

    impl Lrc2 {
        fn step_2x(&mut self, x: f32, f: f32, res: f32) -> f32 {
            let dc = 0.0; // V2: `v2::DC_OFFSET`
            let x = x + dc;
            self.l += f * self.b - dc;
            self.b += f * (x - self.b * res - self.l);
            self.l += f * self.b;
            let h = x - self.b * res - self.l;
            self.b += f * h;
            h
        }
    }

    /// The state-variable filters as they were before the merge (change unify-v2-algorithms 2.1), kept as the
    /// reference: the input form's step (WaveSabre) and the feedback form's (V2's LRC, [`Lrc2`]).
    fn sv_old(kind: Kind, feedback: bool, f: f32, res: f32, x: &[f32]) -> std::vec::Vec<f32> {
        let (mut low, mut band, mut last) = (0.0f32, 0.0f32, 0.0f32);
        let mut lrc = Lrc2 { l: 0.0, b: 0.0 };
        let q = 1.0 - res;
        x.iter()
            .map(|&x| {
                if feedback {
                    let h = lrc.step_2x(x, f, q);
                    return match kind {
                        Kind::SvfLp => lrc.l,
                        Kind::SvfBp => lrc.b,
                        Kind::SvfHp => h,
                        Kind::SvfNotch => lrc.l + h,
                        _ => h + lrc.b + lrc.l,
                    };
                }
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
                let y = (step((last + x) / 2.0) + step(x)) / 2.0;
                last = x;
                y
            })
            .collect()
    }

    /// Gain in dB of a filter (a per-sample function) for a sine at `hz`, after it settles.
    fn sine_db(mut f: impl FnMut(&[f32]) -> std::vec::Vec<f32>, hz: f32) -> f32 {
        let n = 16_384;
        let x: std::vec::Vec<f32> = (0..n).map(|i| 0.25 * libm::sinf(core::f32::consts::TAU * hz * i as f32 / RATE)).collect();
        let y = f(&x);
        let e = |v: &[f32]| v[n / 2..].iter().map(|a| (*a as f64).powi(2)).sum::<f64>();
        (10.0 * (e(&y) / e(&x)).log10()) as f32
    }

    /// Octave centres, 63 Hz – 16 kHz.
    const OCTAVES: [f32; 9] = [63.0, 125.0, 250.0, 500.0, 1000.0, 2000.0, 4000.0, 8000.0, 16_000.0];

    /// The frequency where a low-pass is 3 dB below its 63 Hz level (bisection on a log scale).
    fn corner(mut f: impl FnMut(&[f32]) -> std::vec::Vec<f32>) -> f32 {
        let pass = sine_db(&mut f, 63.0);
        let (mut lo, mut hi) = (63.0f32, 20_000.0f32);
        for _ in 0..24 {
            let mid = libm::sqrtf(lo * hi);
            if sine_db(&mut f, mid) > pass - 3.0 { lo = mid } else { hi = mid }
        }
        libm::sqrtf(lo * hi)
    }

    fn run_c(kind: Kind, c: [f32; NC], x: &[f32]) -> std::vec::Vec<f32> {
        let mut s = Slot::new();
        s.set(kind, &c);
        x.iter().map(|v| s.process(*v, *v).0).collect()
    }

    #[test]
    #[cfg(all(feature = "us-flt-svf", feature = "us-flt-feedback"))]
    fn forms_reproduce_the_earlier_filters() {
        let x = noise(4000);
        for kind in [Kind::SvfLp, Kind::SvfBp, Kind::SvfHp, Kind::SvfNotch, Kind::SvfAll] {
            for (feedback, cut, res) in [(false, 800.0f32, 0.0f32), (false, 3000.0, 0.6), (true, 0.4, 0.0), (true, 0.6, 0.7)] {
                if !feedback && kind == Kind::SvfAll {
                    continue; // no earlier input-form all-pass
                }
                let (f, form) = if feedback { (sv_f_steps(cut, RATE), SvForm::FEEDBACK) } else { (sv_f_hz(cut, RATE), SvForm::INPUT) };
                let c = sv_coefs(f, res, form);
                let new = |v: &[f32]| run_c(kind, c, v);
                let old = |v: &[f32]| sv_old(kind, feedback, f, res, v);
                for hz in OCTAVES {
                    let (a, b) = (sine_db(new, hz), sine_db(old, hz));
                    assert!((a - b).abs() <= 0.5, "{kind:?} feedback {feedback} {cut}/{res} at {hz} Hz: {a} vs {b} dB");
                }
                if kind == Kind::SvfLp && res == 0.0 {
                    let (a, b) = (corner(new), corner(old));
                    assert!((a / b - 1.0).abs() <= 0.03, "{kind:?} feedback {feedback}: corner {a} vs {b} Hz");
                }
                // the merge is exact for both forms: the same samples, not just the same bands
                assert_eq!(new(&x), old(&x), "{kind:?} feedback {feedback} {cut}/{res}");
            }
        }
    }

    #[test]
    #[cfg(all(feature = "us-flt-svf", feature = "us-flt-feedback"))]
    fn forms_combine_freely() {
        // feedback resonance, Hz law, all-pass response, zero resonance: flat
        let c = sv_coefs(sv_f_hz(1000.0, RATE), 0.0, SvForm::FEEDBACK);
        for hz in OCTAVES {
            let g = sine_db(|v| run_c(Kind::SvfAll, c, v), hz);
            assert!(g.abs() <= 0.5, "all-pass at {hz} Hz: {g} dB");
        }
        // every form on every response runs, and the low-pass keeps its slope (−9 dB an octave above the corner)
        for feedback in [false, true] {
            for held in [false, true] {
                let c = sv_coefs(sv_f_hz(1000.0 / 0.75, RATE), 0.3, SvForm { feedback, held });
                for kind in [Kind::SvfLp, Kind::SvfBp, Kind::SvfHp, Kind::SvfNotch, Kind::SvfAll] {
                    assert!(run_c(kind, c, &noise(2000)).iter().all(|v| v.is_finite()), "{kind:?} {feedback} {held}");
                }
                let lp = |hz| sine_db(|v| run_c(Kind::SvfLp, c, v), hz);
                assert!(lp(2000.0) < lp(500.0) - 9.0, "low-pass {feedback} {held}: {} {}", lp(500.0), lp(2000.0));
            }
        }
    }

    /// The 4-pole filters as they were before the merge (change unify-v2-algorithms 2.2), kept as the reference: the
    /// ladder's step (Tunefish) and the 2× step (V2's Moog filter: `v2::Moog` and `v2::flt_coefs` before the change).
    fn ladder_old(hz: f32, q: f32, x: &[f32]) -> std::vec::Vec<f32> {
        let f = 2.0 * hz / RATE;
        let k = 3.6 * f - 1.6 * f * f - 1.0;
        let p = (k + 1.0) * 0.5;
        let res = q.max(0.0).min(0.85) * libm::expf((1.0 - p) * 1.386249);
        let mut s = [0.0f32; 8];
        x.iter()
            .map(|&x| {
                let mut inp = x - res * s[3];
                for i in 0..4 {
                    let y = (s[4 + i] * p + inp * p) - k * s[i];
                    s[4 + i] = inp;
                    s[i] = y;
                    inp = y;
                }
                s[3] - s[3] * s[3] * s[3] * (1.0 / 6.0)
            })
            .collect()
    }

    #[cfg(feature = "us-flt-feedback")]
    pub(crate) fn moog_old(cut: f32, reso: f32, hp: bool, x: &[f32]) -> std::vec::Vec<f32> {
        // V2Flt::set for the Moog modes (`syFltSet`'s operand order)
        let f = crate::v2::calcfreq(cut) * crate::v2::linfreq(RATE) * 0.25;
        let t = 1.0 - f;
        let p = f + 0.8 * (f * t);
        let qx = 5.6 * (t * t) + (1.0 - t);
        let qb = reso * (1.0 + 0.5 * (t * qx));
        let q2 = qb + qb;
        let (f, q) = (1.0 - (p + p), q2 + q2);
        let dc = 0.0; // V2: `v2::DC_OFFSET` (dropped by the merged filter, 6.4)
        let mut b = [0.0f32; 5];
        // V2Moog::step: biased input minus the feedback, four poles, cubic clip, un-biased once
        let mut step = |real: f32| {
            let x = real + dc - q * b[4];
            let t1 = b[1];
            b[1] = (x + b[0]) * p + b[1] * f;
            let t2 = b[2];
            b[2] = (t1 + b[1]) * p + b[2] * f;
            let t3 = b[3];
            b[3] = (t2 + b[2]) * p + b[3] * f;
            let mut b4 = (t3 + b[3]) * p + b[4] * f;
            b4 -= b4 * b4 * b4 * (1.0 / 6.0);
            b4 -= dc;
            b[4] = b4;
            b[0] = real;
            b4
        };
        x.iter()
            .map(|&x| {
                step(x);
                let y = step(x);
                if hp { x - y } else { y }
            })
            .collect()
    }

    #[test]
    #[cfg(all(feature = "us-flt-ladder", feature = "us-flt-feedback"))]
    fn four_pole_forms_reproduce_the_earlier_filters() {
        let x = noise(4000);
        for (hz, q) in [(300.0f32, 0.0f32), (1500.0, 0.5), (6000.0, 0.85)] {
            let c = p4_coefs(p4_f_hz(hz, RATE, P4Form::ONE), q, P4Form::ONE);
            for at in OCTAVES {
                let (a, b) = (sine_db(|v| run_c(Kind::Pole4Lp, c, v), at), sine_db(|v| ladder_old(hz, q, v), at));
                assert!((a - b).abs() <= 0.5, "ladder {hz}/{q} at {at} Hz: {a} vs {b} dB");
            }
            // one rounding apart per pole (`(in + prev)·p` for `prev·p + in·p`)
            assert!(worst(&run_c(Kind::Pole4Lp, c, &x), &ladder_old(hz, q, &x)) < 1e-4, "ladder {hz}/{q}");
        }
        for (cut, reso) in [(0.3f32, 0.0f32), (0.55, 0.5), (0.8, 0.9)] {
            let c = p4_coefs(p4_f_steps(cut, RATE, P4Form::TWO), reso, P4Form::TWO);
            for (kind, hp) in [(Kind::Pole4Lp, false), (Kind::Pole4Hp, true)] {
                for at in OCTAVES {
                    let (a, b) = (sine_db(|v| run_c(kind, c, v), at), sine_db(|v| moog_old(cut, reso, hp, v), at));
                    assert!((a - b).abs() <= 0.5, "{kind:?} {cut}/{reso} at {at} Hz: {a} vs {b} dB");
                }
                assert_eq!(run_c(kind, c, &x), moog_old(cut, reso, hp, &x), "{kind:?} {cut}/{reso}: exact");
            }
        }
    }

    #[test]
    #[cfg(feature = "us-flt-ladder")]
    fn four_pole_high_pass_on_the_hz_law() {
        let c = p4_coefs(p4_f_hz(1000.0, RATE, P4Form::ONE), 0.0, P4Form::ONE);
        // the input minus the low-pass: the low-pass's phase lag limits the rejection a decade down to about 10 dB
        let g = |hz| sine_db(|v| run_c(Kind::Pole4Hp, c, v), hz);
        assert!(g(10_000.0).abs() < 1.0 && g(100.0) <= g(10_000.0) - 6.0, "100 Hz {} dB, 10 kHz {} dB", g(100.0), g(10_000.0));
    }

    #[test]
    #[cfg(all(feature = "us-flt-ladder", feature = "us-flt-feedback"))]
    fn four_pole_forms_combine_freely() {
        for clip_after in [false, true] {
            for x2 in [false, true] {
                let form = P4Form { clip_after, x2 };
                for c in [p4_coefs(p4_f_hz(1000.0, RATE, form), 0.3, form), p4_coefs(p4_f_steps(0.5, RATE, form), 0.3, form)] {
                    assert!(run_c(Kind::Pole4Hp, c, &noise(2000)).iter().all(|v| v.is_finite()), "{form:?}");
                    let lp = |hz| sine_db(|v| run_c(Kind::Pole4Lp, c, v), hz);
                    let corner = corner(|v| run_c(Kind::Pole4Lp, c, v));
                    assert!(lp(corner * 4.0) < lp(63.0) - 24.0, "{form:?} {c:?}: corner {corner}, {} {}", lp(63.0), lp(corner * 4.0));
                }
            }
        }
        // the step count does not move the corner on either law (the coefficients are set for the step rate)
        for law in 0..2 {
            let at = |form| {
                let c = if law == 0 { p4_coefs(p4_f_hz(1000.0, RATE, form), 0.0, form) } else { p4_coefs(p4_f_steps(0.5, RATE, form), 0.0, form) };
                corner(|v| run_c(Kind::Pole4Lp, c, v))
            };
            let (one, two) = (at(P4Form::ONE), at(P4Form { x2: true, ..P4Form::ONE }));
            assert!((one / two - 1.0).abs() < 0.1, "law {law}: 1× corner {one}, 2× {two}");
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
        for kind in [Kind::Pole4Lp, Kind::RbjHp, Kind::RbjBp, Kind::Notch] {
            assert_eq!(coefs(kind, 1000.0, 0.9, RATE), coefs(kind, 1000.0, 0.85, RATE), "{kind:?}");
            assert_ne!(coefs(kind, 1000.0, 0.5, RATE), coefs(kind, 1000.0, 0.1, RATE), "{kind:?}");
        }
        // ladder feedback r = q·e^((1 − p)·1.386249) with p from the cutoff
        let c = coefs(Kind::Pole4Lp, 2205.0, 0.5, RATE);
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
            for k in [Kind::SvfLp, Kind::Pole4Lp] {
                assert!(g(k, 0.0, lo) > -4.0 && g(k, 0.0, hi) < g(k, 0.0, lo) - 9.0, "{k:?} @{rate}: {} {}", g(k, 0.0, lo), g(k, 0.0, hi));
            }
            assert!(g(Kind::Pole4Lp, 0.0, 4000.0) < -20.0, "four poles");
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
        let ladder = |q: f32| (6..=12).map(|k| gain(Kind::Pole4Lp, 1000.0, q, RATE, 100.0 * k as f32)).fold(0.0f32, f32::max);
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
        f.to(Kind::Pole4Lp, &coefs(Kind::Pole4Lp, 800.0, 0.2, RATE), 32);
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

    #[cfg(feature = "us-env-seconds")] // the default patch
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
        for k in [Kind::SvfLp, Kind::Pole4Lp, Kind::RbjHp, Kind::RbjBp, Kind::Notch] {
            assert_eq!(coefs(k, 1.0e6, 0.3, 22050.0), coefs(k, 0.49 * 22050.0, 0.3, 22050.0), "{k:?}");
            assert_eq!(coefs(k, -5.0, 0.3, 22050.0), coefs(k, 1.0, 0.3, 22050.0), "{k:?}: a negative cutoff");
        }
        for k in [Kind::SvfLp, Kind::Pole4Lp, Kind::RbjHp, Kind::Notch] {
            for c in coefs(k, 100_000.0, 0.5, 22050.0) {
                assert!(c.is_finite(), "{k:?}");
            }
        }
        // the 4-pole filter on the Hz law (the ladder) at full cutoff and resonance is wild at any rate (about 10 on unit
        // noise at 44.1 kHz, like Tunefish's); without `us-flt-ladder` its Hz-law coefficients are not built
        if cfg!(feature = "us-flt-ladder") {
            let y = run(Kind::Pole4Lp, 19_980.0, 0.85, 22050.0, &noise(20_000));
            let peak = |v: &[f32]| v.iter().fold(0.0f32, |m, a| m.max(a.abs()));
            let y44 = run(Kind::Pole4Lp, 19_980.0, 0.85, 44100.0, &noise(20_000));
            assert!(y.iter().all(|v| v.is_finite()) && peak(&y) < 2.0 * peak(&y44), "{} {}", peak(&y), peak(&y44));
        }
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
            (Kind::Pole4Lp, OPEN_LP_HZ, 0.0, true, "ladder"),
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
                Kind::Pole4Lp => cfg!(feature = "us-flt-ladder"),
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

    #[cfg(all(feature = "probe", feature = "us-flt-svf"))]
    #[test]
    fn the_editor_can_run_open_filters_for_listening() {
        assert!(transparent(Kind::SvfLp, OPEN_LP_HZ, 0.0));
        set_run_open(true);
        assert!(!transparent(Kind::SvfLp, OPEN_LP_HZ, 0.0), "run, as the legacy engines do");
        set_run_open(false);
        assert!(transparent(Kind::SvfLp, OPEN_LP_HZ, 0.0));
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
            assert!(open_gain_db(Kind::Pole4Lp, OPEN_LP_HZ, 1000.0, rate, 0.05).abs() < 0.02);
            let full = open_gain_db(Kind::Pole4Lp, OPEN_LP_HZ, 1000.0, rate, 1.0);
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
        let ladder = wake_gap(Kind::Pole4Lp, OPEN_LP_HZ, 5000.0, true);
        assert!(ladder < 0.03, "ladder primed: {ladder}");
        // high-pass at the bottom → high-pass at 200 Hz
        let hp = wake_gap(Kind::RbjHp, OPEN_HP_HZ, 200.0, true);
        assert!(hp < 0.03, "rbj high-pass primed: {hp}");
        // the priming is what does it: from a zero state the gap is several times larger
        let cold = wake_gap(Kind::SvfLp, OPEN_LP_HZ, 5000.0, false);
        assert!(cold > 5.0 * primed.max(0.01), "cold {cold} against primed {primed}");
    }
}
