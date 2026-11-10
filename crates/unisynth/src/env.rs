//! ADSR envelopes (design D1, change unify-v2-algorithms 4.1): one stage machine for every law, advanced by elapsed
//! time at control rate.
//!
//! - The **time law** turns the attack, decay and release fields into a time constant per stage: `seconds`
//!   (`law::env_ms`, WaveSabre's), `rate` (Tunefish's per-512-block log-rate steps per second: attack
//!   `−log₁₀(0.94·a³)`, decay `¼·log₁₀(0.94·(d·M)³)`, release `¼·log₁₀(0.94·r³)`), or `frame` (V2's per-frame steps at
//!   344.53 frames per second: attack `2^(7 − 0.09375·ar)` per frame of 128, decay and release factors
//!   `1 − calcfreq2(1 − x)`).
//! - The **attack** is linear in level for every law, from the level at note-on (0, or the current level on the
//!   frame law) to 1.
//! - The **curve** shapes decay and release: `quadratic` (`(1 − t/T)²`, closed form), `power` (Tunefish's
//!   `((0.01 + v − S)/(1 − S))^slope` and `v^slope` rates, one Euler step per control segment; the sustain level is at
//!   most 0.99), `exponential` (a constant factor per second toward 0, stopping at the sustain level; base 2).
//! - The **sustain slope** multiplies the held level by `2^(0.0019375·(128·slope − 64)·344.53)` per second (falls below
//!   ½, rises above, capped at 1), and the **amplify** scales the output.
//! - An envelope ends at the end of a quadratic release, below 10⁻⁵ in a power release, or below `2⁻¹³/128` in any
//!   exponential stage.
//!
//! The fields (v3 layout): law, curve ("by law": quadratic for seconds, power for rate, exponential for frame),
//! curve slope (the power exponent), sustain slope, amplify. The frame law is re-read every control segment and keeps
//! the level at note-on; the other laws take their fields at note-on, like the original envelopes. The power curve's
//! code comes with `us-env-rate`, the exponential curve's, the sustain slope's and amplify's with `us-env-frame`.

#[cfg(feature = "us-env-seconds")]
use crate::law;
use crate::params::{self as pm, e};

/// Rate-law rates are per 512-frame block at 44 100 Hz; this many blocks per second.
#[cfg(feature = "us-env-rate")]
const TF_BLOCKS_PER_S: f32 = 44_100.0 / 512.0;
/// Rate-law per-block step scale `0.0005·512`.
#[cfg(feature = "us-env-rate")]
const TF_STEP: f32 = 0.0005 * 512.0;
/// A power release ends below this level.
const POWER_DONE: f32 = 1e-5;
/// The settings beyond the old modes (sustain slope, amplify, the frame law, the exponential curve).
const FRAME: bool = cfg!(feature = "us-env-frame");

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stage {
    Attack,
    Decay,
    Sustain,
    Release,
    Finished,
}

/// Time laws.
pub mod time_law {
    pub const SECONDS: u8 = 0;
    pub const RATE: u8 = 1;
    pub const FRAME: u8 = 2;
}

/// Decay and release curves.
pub mod curve {
    pub const QUADRATIC: u8 = 0;
    pub const POWER: u8 = 1;
    pub const EXPONENTIAL: u8 = 2;
}

/// An envelope's settings and fields (read at note-on, or every control segment on the frame law).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EnvP {
    pub law: u8,
    pub curve: u8,
    pub a: f32,
    pub d: f32,
    pub s: f32,
    pub r: f32,
    /// The power curve's exponent.
    pub curve_slope: f32,
    /// 0..1, ½ keeps the held level.
    pub sustain_slope: f32,
    pub amp: f32,
    /// Frame law with quirk `coarse_env`: 256-sample frames and the year-2000 laws (v2redux `DELTA_FRAME256`,
    /// `DELTA_ENV_CURVES`).
    pub coarse: bool,
    /// A seconds-law attack of 0 is instant instead of 1 ms: a voice with a clocked slot sets it (the source has no
    /// attack; change add-cauldron-xm-import).
    pub instant: bool,
    /// Elevated's linear timed envelope (change add-elevated-import D4): attack, decay, hold (`hold`) and release run
    /// on their own, linear in level, times `law::lin_ms`; a note-off ends it at once. Only with `us-env-linear`.
    #[cfg(feature = "us-env-linear")]
    pub linear: bool,
    #[cfg(feature = "us-env-linear")]
    pub hold: f32,
}

/// The law this build runs for a requested seconds or rate law: a build with one of them runs it for both (a patch that
/// asks for the other fails at load; only automating the mode can get here).
const fn of_mode(by_rate: bool) -> bool {
    if cfg!(all(feature = "us-env-seconds", feature = "us-env-rate")) {
        by_rate
    } else {
        cfg!(feature = "us-env-rate")
    }
}

/// Fails with `UnsupportedModule(ENV_FRAME)` for a frame-law envelope in a build without `us-env-frame`.
pub fn check_frame(frame: bool) -> Result<(), synth_core::mix::LoadError> {
    if frame && !FRAME {
        return Err(synth_core::mix::LoadError::UnsupportedModule(crate::module::ENV_FRAME));
    }
    Ok(())
}

/// Fails with `UnsupportedModule` when the build lacks what envelope `i` of `p` uses: its law, its curve (power: the
/// rate law's module, exponential: the frame law's), a sustain slope or an amplify (the frame law's module).
pub fn check_settings(p: &[f32; pm::COUNT], i: usize) -> Result<(), synth_core::mix::LoadError> {
    let lw = crate::law::choice(p[pm::env(i, e::LAW)], pm::env_law::COUNT);
    let c = crate::law::choice(p[pm::env(i, e::CURVE)], pm::env_curve::COUNT);
    let c = if c == pm::env_curve::BY_LAW { lw + 1 } else { c };
    let frame_code = lw == pm::env_law::FRAME
        || c == pm::env_curve::EXPONENTIAL
        || p[pm::env(i, e::SUSTAIN_SLOPE)] != 0.5
        || p[pm::env(i, e::AMPLIFY)] != 1.0;
    check_frame(frame_code)?;
    if lw != pm::env_law::FRAME {
        check(lw == pm::env_law::RATE)?;
    }
    if c == pm::env_curve::POWER {
        check(true)?;
    }
    Ok(())
}

/// Fails with `UnsupportedModule` when the build lacks the law `by_rate` asks for.
pub fn check(by_rate: bool) -> Result<(), synth_core::mix::LoadError> {
    let (have, id) = if by_rate {
        (cfg!(feature = "us-env-rate"), crate::module::ENV_RATE)
    } else {
        (cfg!(feature = "us-env-seconds"), crate::module::ENV_SECONDS)
    };
    if have {
        Ok(())
    } else {
        Err(synth_core::mix::LoadError::UnsupportedModule(id))
    }
}

impl EnvP {
    /// A linear timed envelope (always false without `us-env-linear`).
    #[inline]
    pub fn is_linear(&self) -> bool {
        #[cfg(feature = "us-env-linear")]
        return self.linear;
        #[cfg(not(feature = "us-env-linear"))]
        false
    }

    pub const DEFAULT: EnvP = EnvP {
        law: time_law::SECONDS,
        curve: curve::QUADRATIC,
        a: 0.0,
        d: 0.3,
        s: 1.0,
        r: 0.1,
        curve_slope: 0.5,
        sustain_slope: 0.5,
        amp: 1.0,
        coarse: false,
        instant: false,
        #[cfg(feature = "us-env-linear")]
        linear: false,
        #[cfg(feature = "us-env-linear")]
        hold: 0.0,
    };

    pub fn read(p: &[f32; pm::COUNT], i: usize) -> Self {
        let lw = crate::law::choice(p[pm::env(i, e::LAW)], pm::env_law::COUNT);
        let law = if FRAME && lw == pm::env_law::FRAME {
            time_law::FRAME
        } else if of_mode(lw == pm::env_law::RATE) {
            time_law::RATE
        } else {
            time_law::SECONDS
        };
        let frame = law == time_law::FRAME;
        // only the power and exponential curves' modules read the curve; without them it is the quadratic
        let curve = if cfg!(any(feature = "us-env-rate", feature = "us-env-frame")) {
            let c = crate::law::choice(p[pm::env(i, e::CURVE)], pm::env_curve::COUNT);
            if c == pm::env_curve::BY_LAW { law } else { (c - 1) as u8 }
        } else {
            curve::QUADRATIC
        };
        EnvP {
            law,
            curve,
            a: p[pm::env(i, e::ATTACK)],
            d: p[pm::env(i, e::DECAY)],
            s: p[pm::env(i, e::SUSTAIN)],
            r: p[pm::env(i, e::RELEASE)],
            curve_slope: p[pm::env(i, e::CURVE_SLOPE)],
            sustain_slope: if FRAME { p[pm::env(i, e::SUSTAIN_SLOPE)] } else { 0.5 },
            amp: if FRAME { p[pm::env(i, e::AMPLIFY)] } else { 1.0 },
            coarse: frame && cfg!(feature = "us-quirk-coarse-env") && pm::quirk_on(p, pm::quirk::COARSE_ENV),
            instant: false,
            #[cfg(feature = "us-env-linear")]
            linear: crate::law::choice(p[pm::el_env(i, pm::el::LINEAR)], 2) == 1,
            #[cfg(feature = "us-env-linear")]
            hold: p[pm::el_env(i, pm::el::HOLD)],
        }
    }

    /// Re-read every control segment (the frame law: the matrix may move it).
    #[inline]
    pub fn live(&self) -> bool {
        FRAME && self.law == time_law::FRAME
    }

    /// A note-on starts the attack from the current level (the frame law, unless the voice resets fully).
    #[inline]
    pub fn keeps_level(&self) -> bool {
        FRAME && self.law == time_law::FRAME
    }
}

/// Seconds of a seconds-law time field.
#[inline]
#[cfg(feature = "us-env-seconds")]
fn ws_s(p: f32) -> f32 {
    law::env_ms(p) * 0.001
}

/// The time constant of a decay or release (`release`) or of the attack's full rise (`attack`), seconds, for the law
/// (quadratic: the stage's length; power: `1/rate`, negative for a rising power decay; exponential: the time per halving,
/// in base 2). Computed only for the stage that runs, as the original envelopes do.
#[inline]
fn time(ep: &EnvP, stage: Stage, decay_mod: f32) -> f32 {
    let _ = decay_mod;
    #[cfg(feature = "us-env-frame")]
    if ep.law == time_law::FRAME {
        use crate::v2::{calcfreq, calcfreq2, FRAMES_PER_S};
        let raw = |x: f32| x * 128.0;
        // format versions below 2 step every 256 samples with the year-2000 laws
        let (per_s, amul, rate) = if ep.coarse { (FRAMES_PER_S * 0.5, -0.085_937_5, calcfreq as fn(f32) -> f32) } else { (FRAMES_PER_S, -0.093_75, calcfreq2 as fn(f32) -> f32) };
        // a factor per frame → seconds per halving (a factor ≤ 0 reaches the target at once)
        let half = |f: f32| if f <= 0.0 { 0.0 } else { -1.0 / (crate::fm::log2(f as f64) as f32 * per_s) };
        return match stage {
            Stage::Attack => 128.0 / (crate::fm::exp2f(raw(ep.a) * amul + 7.0) * per_s),
            Stage::Decay => half(1.0 - rate(1.0 - ep.d)),
            _ => half(1.0 - rate(1.0 - ep.r)),
        };
    }
    #[cfg(feature = "us-env-rate")]
    if ep.law == time_law::RATE {
        let cube = |x: f32| x * x * x;
        let per_s = |x: f32| crate::fm::log10f(cube(x).max(1e-9) * 0.94) * TF_STEP * TF_BLOCKS_PER_S;
        return match stage {
            Stage::Attack => -1.0 / per_s(ep.a),
            Stage::Decay => -1.0 / (0.25 * per_s(ep.d * decay_mod)),
            _ => -1.0 / (0.25 * per_s(ep.r)),
        };
    }
    #[cfg(feature = "us-env-seconds")]
    return match stage {
        Stage::Attack if ep.instant && ep.a == 0.0 => 0.0,
        Stage::Attack => ws_s(ep.a),
        Stage::Decay => ws_s(ep.d),
        _ => ws_s(ep.r),
    };
    #[allow(unreachable_code)]
    {
        let _ = (ep, stage);
        0.0
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Env {
    stage: Stage,
    /// Seconds into the stage.
    pos: f32,
    /// The level, 0..1.
    v: f32,
    /// The level the attack or the release started from.
    from: f32,
    /// The output, `v · amplify` (`v2::Opt`: absent without the frame law; the output is then `v`).
    #[cfg_attr(not(feature = "us-env-frame"), allow(dead_code))]
    out: crate::v2::Opt<f32>,
}

impl Env {
    pub const fn new() -> Self {
        Env { stage: Stage::Finished, pos: 0.0, v: 0.0, from: 0.0, out: crate::v2::Opt::new(0.0) }
    }

    /// Note-on: the attack starts from 0, or from the current level when `keep` ([`EnvP::keeps_level`]).
    pub fn trigger(&mut self, keep: bool) {
        if FRAME && keep {
            self.stage = Stage::Attack;
            self.pos = 0.0;
            self.from = self.v;
        } else {
            *self = Env { stage: Stage::Attack, pos: 0.0, v: 0.0, from: 0.0, out: crate::v2::Opt::new(0.0) };
        }
    }

    /// A note-off: a linear timed envelope ends at once (Elevated's cut, change add-elevated-import D4), any other
    /// releases ([`Env::release`]).
    pub fn note_off(&mut self, ep: &EnvP) {
        if ep.is_linear() {
            *self = Env::new();
        } else {
            self.release();
        }
    }

    /// A linear timed envelope's frames to the end of its stage at `rate` (rounded up, at least 1), `None` for another
    /// envelope or a finished one: the voice ends its control segment there, so the stage corners are frame-exact
    /// (a 3-frame attack keeps its click; change add-elevated-import).
    #[cfg(feature = "us-env-linear")]
    pub fn linear_left(&self, ep: &EnvP, rate: f32) -> Option<u32> {
        if !ep.linear {
            return None;
        }
        let secs = |p: f32| crate::law::lin_ms(p) * 0.001;
        let len = match self.stage {
            Stage::Attack => secs(ep.a),
            Stage::Decay => secs(ep.d),
            Stage::Sustain => secs(ep.hold),
            Stage::Release => secs(ep.r),
            Stage::Finished => return None,
        };
        Some((libm::ceilf((len - self.pos) * rate - 1e-3)).max(1.0) as u32)
    }

    /// The linear timed envelope: each stage linear in level over its `law::lin_ms` time, a zero-length stage skipped;
    /// the hold keeps the sustain level for `hold`, then the release falls from it to 0 and the envelope ends.
    #[cfg(feature = "us-env-linear")]
    fn advance_linear(&mut self, mut t: f32, ep: &EnvP) {
        let secs = |p: f32| crate::law::lin_ms(p) * 0.001;
        loop {
            let (len, from, to, next) = match self.stage {
                Stage::Attack => (secs(ep.a), 0.0, 1.0, Stage::Decay),
                Stage::Decay => (secs(ep.d), 1.0, ep.s, Stage::Sustain),
                Stage::Sustain => (secs(ep.hold), ep.s, ep.s, Stage::Release),
                Stage::Release => (secs(ep.r), ep.s, 0.0, Stage::Finished),
                Stage::Finished => {
                    *self = Env::new();
                    return;
                }
            };
            if t >= len - self.pos {
                t -= (len - self.pos).max(0.0);
                self.stage = next;
                self.pos = 0.0;
                self.v = to;
            } else {
                self.pos += t;
                self.v = from + (to - from) * self.pos / len;
                return;
            }
        }
    }

    /// Starts the release from the current level (no effect before the note or once released).
    pub fn release(&mut self) {
        if !matches!(self.stage, Stage::Release | Stage::Finished) {
            self.stage = Stage::Release;
            self.pos = 0.0;
            self.from = self.v;
        }
    }

    #[inline]
    pub fn value(&self) -> f32 {
        if FRAME { self.out.get() } else { self.v }
    }

    #[inline]
    pub fn stage(&self) -> Stage {
        self.stage
    }

    #[inline]
    pub fn finished(&self) -> bool {
        self.stage == Stage::Finished
    }

    /// Advances by `dt` seconds. `decay_mod` scales the rate-law decay (mod matrix `M(decay)`; 1 = none). Kept out of
    /// line: inlined into the voice's control step it packs worse.
    #[inline(never)]
    pub fn advance(&mut self, mut t: f32, ep: &EnvP, decay_mod: f32) {
        #[cfg(feature = "us-env-linear")]
        if ep.linear {
            self.advance_linear(t, ep);
            if FRAME {
                self.out.set(self.v * ep.amp);
            }
            return;
        }
        // each curve's code is built with the law that brings it (until the format switch); a build with one law knows its
        // curve, so the others' code drops out
        let (secs, rate) = (cfg!(feature = "us-env-seconds"), cfg!(feature = "us-env-rate"));
        let power = rate && (ep.curve == curve::POWER || (!secs && !FRAME));
        let expo = FRAME && (ep.curve == curve::EXPONENTIAL || (!secs && !rate));
        // the power curve's sustain level stays below 1 (its decay divides by 1 − S)
        let sus = if power { ep.s.min(0.99) } else { ep.s };
        loop {
            match self.stage {
                Stage::Attack => {
                    let ta = time(ep, Stage::Attack, decay_mod);
                    // without the frame law an attack starts at 0 (the plain seconds and rate arithmetic)
                    let from = if FRAME { self.from } else { 0.0 };
                    let need = ta * (1.0 - from) - self.pos;
                    let need = if cfg!(feature = "us-env-rate") { need.max(0.0) } else { need };
                    if t >= need {
                        self.stage = Stage::Decay;
                        self.pos = 0.0;
                        self.v = 1.0;
                        if power {
                            break; // one Euler step per segment: the decay starts at the next
                        }
                        t -= need;
                    } else {
                        self.pos += t;
                        self.v = if FRAME { self.from + self.pos / ta } else { self.pos / ta };
                        break;
                    }
                }
                Stage::Decay if power => {
                    if self.v < sus {
                        self.v = sus;
                        self.stage = Stage::Sustain;
                    } else {
                        let pos = (0.01 + (self.v - sus)) / (1.0 - sus);
                        self.v -= crate::fm::powf(pos, ep.curve_slope) * t / time(ep, Stage::Decay, decay_mod);
                        if self.v <= sus {
                            self.v = sus;
                            self.stage = Stage::Sustain;
                        }
                        self.v = self.v.min(1.0); // a decay that would rise holds at the peak
                    }
                    break;
                }
                Stage::Decay if expo => {
                    let td = time(ep, Stage::Decay, decay_mod);
                    if self.v <= sus || td <= 0.0 {
                        self.v = if td <= 0.0 { sus } else { self.v.min(sus) };
                        self.stage = Stage::Sustain;
                        continue;
                    }
                    // seconds until v·2^(−t/τ) reaches the sustain level
                    let need = td * crate::fm::log2((self.v / sus.max(1e-30)) as f64) as f32;
                    if t >= need {
                        t -= need;
                        self.v = sus;
                        self.stage = Stage::Sustain;
                    } else {
                        self.v *= crate::fm::exp2f(-t / td);
                        break;
                    }
                }
                Stage::Decay => {
                    let td = time(ep, Stage::Decay, decay_mod);
                    if t >= td - self.pos {
                        if FRAME {
                            t -= td - self.pos; // a sloped sustain uses the rest
                        }
                        self.stage = Stage::Sustain;
                        self.v = sus;
                    } else {
                        self.pos += t;
                        let f = 1.0 - self.pos / td;
                        let f = f * f;
                        self.v = f + sus * (1.0 - f);
                        break;
                    }
                }
                Stage::Sustain => {
                    if FRAME && ep.sustain_slope != 0.5 {
                        let k = 0.001_937_5 * crate::v2::FRAMES_PER_S * if ep.coarse { 0.5 } else { 1.0 };
                        self.v = (self.v * crate::fm::exp2f(k * (ep.sustain_slope * 128.0 - 64.0) * t)).min(1.0);
                    } else if !expo {
                        self.v = sus;
                    }
                    break;
                }
                Stage::Release => {
                    let tr = time(ep, Stage::Release, decay_mod);
                    if power {
                        self.v -= crate::fm::powf(self.v.max(0.0), ep.curve_slope) * t / tr;
                        if self.v <= POWER_DONE {
                            *self = Env::new();
                        }
                    } else if expo {
                        self.v *= if tr <= 0.0 { 0.0 } else { crate::fm::exp2f(-t / tr) };
                    } else if t >= tr - self.pos {
                        *self = Env::new();
                    } else {
                        self.pos += t;
                        let f = 1.0 - self.pos / tr;
                        self.v = self.from * f * f;
                    }
                    break;
                }
                Stage::Finished => {
                    self.v = 0.0;
                    break;
                }
            }
        }
        if expo && self.v * 128.0 <= crate::v2::LOWEST {
            *self = Env::new();
        }
        if FRAME {
            self.out.set(self.v * ep.amp);
        }
    }
}

impl Default for Env {
    fn default() -> Self {
        Env::new()
    }
}

#[cfg(all(test, feature = "us-env-seconds", feature = "us-env-rate"))]
mod tests {
    use super::*;

    /// The envelope modes as they were before the merge (change unify-v2-algorithms 4.1), kept as the reference:
    /// `advance_ws` (timed), `advance_tf` (rate) and `advance_v2` (V2), on the earlier fields.
    mod old {
        use super::super::{Stage, TF_BLOCKS_PER_S, TF_STEP};
        pub struct OldP {
            pub a: f32,
            pub d: f32,
            pub s: f32,
            pub r: f32,
            pub slope: f32,
            pub instant: bool,
            pub v2_old: bool,
            pub amp: f32,
        }
        const TF_DONE: f32 = 1e-5;
        #[derive(Clone, Copy)]
        pub struct OldEnv {
            pub stage: Stage,
            pos: f32,
            pub v: f32,
            from: f32,
            val: f32,
        }
        fn ws_s(p: f32) -> f32 {
            crate::law::env_ms(p) * 0.001
        }
        impl OldEnv {
            pub fn new() -> Self {
                OldEnv { stage: Stage::Finished, pos: 0.0, v: 0.0, from: 0.0, val: 0.0 }
            }
            pub fn trigger() -> Self {
                OldEnv { stage: Stage::Attack, ..OldEnv::new() }
            }
            pub fn release(&mut self) {
                if !matches!(self.stage, Stage::Release | Stage::Finished) {
                    self.stage = Stage::Release;
                    self.pos = 0.0;
                    self.from = self.v;
                }
            }
            pub fn advance_ws(&mut self, mut t: f32, ep: &OldP) {
        loop {
            match self.stage {
                Stage::Attack => {
                    let a = if ep.instant && ep.a == 0.0 { 0.0 } else { ws_s(ep.a) };
                    if t >= a - self.pos {
                        t -= a - self.pos;
                        self.stage = Stage::Decay;
                        self.pos = 0.0;
                        self.v = 1.0;
                    } else {
                        self.pos += t;
                        self.v = self.pos / a;
                        return;
                    }
                }
                Stage::Decay => {
                    let d = ws_s(ep.d);
                    if t >= d - self.pos {
                        self.stage = Stage::Sustain;
                        self.v = ep.s;
                    } else {
                        self.pos += t;
                        let f = 1.0 - self.pos / d;
                        let f = f * f;
                        self.v = f + ep.s * (1.0 - f);
                        return;
                    }
                }
                Stage::Sustain => {
                    self.v = ep.s;
                    return;
                }
                Stage::Release => {
                    let r = ws_s(ep.r);
                    if t >= r - self.pos {
                        *self = OldEnv::new();
                    } else {
                        self.pos += t;
                        let f = 1.0 - self.pos / r;
                        self.v = self.from * f * f;
                    }
                    return;
                }
                Stage::Finished => {
                    self.v = 0.0;
                    return;
                }
            }
        }
    }
            pub fn advance_tf(&mut self, dt: f32, ep: &OldP, decay_mod: f32) {
        let cube = |x: f32| x * x * x;
        let blocks = dt * TF_BLOCKS_PER_S;
        let sustain = ep.s.min(0.99);
        match self.stage {
            Stage::Attack => {
                let rate = -crate::fm::log10f((cube(ep.a).max(1e-9)) * 0.94) * TF_STEP;
                self.v += rate * blocks;
                if self.v >= 1.0 {
                    self.v = 1.0;
                    self.stage = Stage::Decay;
                }
            }
            Stage::Decay => {
                if self.v < sustain {
                    self.v = sustain;
                    self.stage = Stage::Sustain;
                } else {
                    let rate = crate::fm::log10f(cube(ep.d * decay_mod).max(1e-9) * 0.94) * 0.25 * TF_STEP;
                    let pos = (0.01 + (self.v - sustain)) / (1.0 - sustain);
                    self.v += rate * crate::fm::powf(pos, ep.slope) * blocks;
                    if self.v <= sustain {
                        self.v = sustain;
                        self.stage = Stage::Sustain;
                    }
                    self.v = self.v.min(1.0); // the original decay can rise past 1; here it holds at the peak
                }
            }
            Stage::Sustain => self.v = sustain,
            Stage::Release => {
                let rate = crate::fm::log10f(cube(ep.r).max(1e-9) * 0.94) * 0.25 * TF_STEP;
                self.v += rate * crate::fm::powf(self.v.max(0.0), ep.slope) * blocks;
                if self.v <= TF_DONE {
                    *self = OldEnv::new();
                }
            }
            Stage::Finished => self.v = 0.0,
        }
    }
            pub fn advance_v2(&mut self, dt: f32, ep: &OldP) {
        use crate::fm::{exp2f, log2, powf};
        use crate::v2::{calcfreq, calcfreq2, FRAMES_PER_S, LOWEST};
        let raw = |x: f32| x * 128.0;
        // format versions below 2 step every 256 samples with the year-2000 laws
        let (per_s, amul, rate) = if ep.v2_old { (FRAMES_PER_S * 0.5, -0.085_937_5, calcfreq as fn(f32) -> f32) } else { (FRAMES_PER_S, -0.093_75, calcfreq2 as fn(f32) -> f32) };
        let mut n = dt * per_s;
        let mut val = self.val;
        loop {
            match self.stage {
                Stage::Attack => {
                    let atd = exp2f(raw(ep.a) * amul + 7.0);
                    let need = (128.0 - val) / atd;
                    if n >= need {
                        n -= need;
                        val = 128.0;
                        self.stage = Stage::Decay;
                    } else {
                        val += atd * n;
                        break;
                    }
                }
                Stage::Decay => {
                    let dcf = 1.0 - rate(1.0 - ep.d);
                    let sul = raw(ep.s);
                    if val <= sul || dcf <= 0.0 {
                        val = val.min(sul).max(if dcf <= 0.0 { sul } else { 0.0 });
                        self.stage = Stage::Sustain;
                        continue;
                    }
                    // frames until val·dcfⁿ reaches sul
                    let need = (log2((sul.max(1e-30) / val) as f64) / log2(dcf as f64)) as f32;
                    if n >= need {
                        n -= need;
                        val = sul;
                        self.stage = Stage::Sustain;
                    } else {
                        val *= powf(dcf, n);
                        break;
                    }
                }
                Stage::Sustain => {
                    val = (val * exp2f(0.001_937_5 * (raw(ep.slope) - 64.0) * n)).min(128.0);
                    break;
                }
                Stage::Release => {
                    let rf = 1.0 - rate(1.0 - ep.r);
                    val *= powf(rf.max(0.0), n);
                    break;
                }
                Stage::Finished => {
                    val = 0.0;
                    break;
                }
            }
        }
        if val <= LOWEST {
            *self = OldEnv::new();
            return;
        }
        self.val = val;
        self.v = val / 128.0 * ep.amp;
    }

        }
    }

    /// The level of an envelope in dB (−∞ below 1e-9).
    fn db(v: f32) -> f32 {
        20.0 * libm::log10f(v.max(1e-9))
    }

    /// Runs the new and the old envelope side by side in 32-frame blocks for `secs` with a note-off at `off`, and
    /// checks the settings that replace an earlier mode: every stage boundary within one block, and the level within
    /// 0.5 dB wherever either is above −60 dB.
    #[cfg(feature = "us-env-frame")]
    fn same_as_before(ep: &EnvP, op: &old::OldP, mode: u8, secs: f32, off: f32) {
        let block = 32.0 / 44_100.0;
        let mut e = Env::new();
        e.trigger(false);
        let mut o = old::OldEnv::trigger();
        let mut last = (e.stage(), o.stage, 0usize, 0usize);
        for b in 0..(secs / block) as usize {
            if (b as f32 * block - off).abs() < block / 2.0 {
                e.release();
                o.release();
            }
            e.advance(block, ep, 1.0);
            match mode {
                0 => o.advance_ws(block, op),
                1 => o.advance_tf(block, op, 1.0),
                _ => o.advance_v2(block, op),
            }
            let (a, w) = (e.value(), o.v);
            if a.max(w) > 1e-3 {
                assert!((db(a) - db(w)).abs() <= 0.5, "mode {mode} block {b}: {a} vs {w}");
            }
            // stage changes: the new one within one block of the old one
            if e.stage() != last.0 {
                last = (e.stage(), last.1, b, last.3);
            }
            if o.stage != last.1 {
                last = (last.0, o.stage, last.2, b);
            }
            if e.stage() == o.stage {
                assert!(last.2.abs_diff(last.3) <= 1, "mode {mode}: stage {:?} at block {} vs {}", e.stage(), last.2, last.3);
            }
        }
    }

    #[test]
    #[cfg(feature = "us-env-frame")]
    fn earlier_modes_as_settings() {
        let inv = crate::law::host::inv_env_ms;
        for (a, d, s, r) in [(5.0f32, 120.0f32, 0.5f32, 300.0f32), (0.0, 40.0, 0.0, 80.0), (300.0, 900.0, 0.8, 1500.0)] {
            let ep = EnvP { a: inv(a), d: inv(d), s, r: inv(r), ..EnvP::DEFAULT };
            let op = old::OldP { a: inv(a), d: inv(d), s, r: inv(r), slope: 0.5, instant: false, v2_old: false, amp: 1.0 };
            same_as_before(&ep, &op, 0, 4.0, 1.5);
        }
        for (a, d, s, r, slope) in [(0.6f32, 0.35f32, 0.45f32, 0.4f32, 0.5f32), (0.2, 0.6, 0.2, 0.7, 1.5), (0.9, 0.2, 0.7, 0.3, 0.2)] {
            let ep = EnvP { law: time_law::RATE, curve: curve::POWER, a, d, s, r, curve_slope: slope, ..EnvP::DEFAULT };
            let op = old::OldP { a, d, s, r, slope, instant: false, v2_old: false, amp: 1.0 };
            same_as_before(&ep, &op, 1, 4.0, 2.0);
        }
        for coarse in [false, true] {
            for (a, d, s, r, sl) in [(10.0f32, 50.0f32, 80.0f32, 60.0f32, 64.0f32), (60.0, 80.0, 40.0, 90.0, 40.0), (0.0, 100.0, 0.0, 40.0, 90.0)] {
                let (a, d, s, r, sl) = (a / 128.0, d / 128.0, s / 128.0, r / 128.0, sl / 128.0);
                let ep = EnvP { law: time_law::FRAME, curve: curve::EXPONENTIAL, a, d, s, r, sustain_slope: sl, amp: 0.8, coarse, ..EnvP::DEFAULT };
                let op = old::OldP { a, d, s, r, slope: sl, instant: false, v2_old: coarse, amp: 0.8 };
                same_as_before(&ep, &op, 2, 4.0, 2.5);
            }
        }
    }

    #[test]
    #[cfg(feature = "us-env-frame")]
    fn sustain_slope_on_a_seconds_law_envelope() {
        // the frame law's slope: 2^(0.0019375·(128·slope − 64)) per frame, ±21 octaves per second at the ends
        let inv = crate::law::host::inv_env_ms;
        let held = |slope: f32| {
            let ep = EnvP { a: inv(5.0), d: inv(50.0), s: 0.6, r: inv(100.0), sustain_slope: slope, ..EnvP::DEFAULT };
            let mut e = Env::new();
            e.trigger(false);
            (run(&mut e, &ep, 0.1, 0.001), run(&mut e, &ep, 0.2, 0.001))
        };
        let (start, end) = held(0.45);
        assert!(end < start * 0.6 && end > 0.0, "falls while held: {start} → {end}");
        assert_eq!(held(0.5), (0.6, 0.6), "the centre keeps the sustain level");
        let (start, end) = held(0.55);
        assert!(end > start && end <= 1.0, "rises above the centre: {start} → {end}");
    }

    const WS: EnvP = EnvP { a: 0.0, d: 0.0, s: 0.5, r: 0.0, curve_slope: 0.5, ..EnvP::DEFAULT };

    /// Advances `t` seconds in steps of `step`, returns the value.
    fn run(e: &mut Env, ep: &EnvP, t: f32, step: f32) -> f32 {
        let n = (t / step).round() as usize;
        for _ in 0..n {
            e.advance(step, ep, 1.0);
        }
        e.value()
    }

    #[test]
    fn an_instant_attack_starts_at_full_level() {
        // a clocked voice's timed attack of 0 (the original has no attack): full at once, then the decay as usual
        let ep = EnvP { instant: true, s: 1.0, ..WS };
        let mut e = Env::new();
        e.trigger(false);
        e.advance(1e-6, &ep, 1.0);
        assert_eq!(e.value(), 1.0);
        let mut e = Env::new();
        e.trigger(false);
        e.advance(1e-6, &EnvP { s: 1.0, ..WS }, 1.0);
        assert!(e.value() < 0.01, "a plain attack of 0 is 1 ms: {}", e.value());
    }

    fn ep_ms(a_ms: f32, d_ms: f32, s: f32, r_ms: f32) -> EnvP {
        let inv = |ms: f32| crate::law::host::inv_env_ms(ms);
        EnvP { a: inv(a_ms), d: inv(d_ms), s, r: inv(r_ms), curve_slope: 0.5, ..EnvP::DEFAULT }
    }

    #[test]
    fn ws_curves_match_the_original_formulas() {
        let ep = ep_ms(100.0, 200.0, 0.4, 300.0);
        let mut e = Env::new();
        assert_eq!(e.value(), 0.0);
        e.trigger(false);
        assert_eq!(e.value(), 0.0);
        assert!((run(&mut e, &ep, 0.05, 0.001) - 0.5).abs() < 1e-4, "attack is linear");
        assert!((run(&mut e, &ep, 0.05, 0.001) - 1.0).abs() < 1e-3);
        assert_eq!(e.stage(), Stage::Decay);
        // 100 ms into a 200 ms decay: f = (1 − 0.5)² = 0.25 → 1·0.25 + 0.4·0.75
        assert!((run(&mut e, &ep, 0.1, 0.001) - (0.25 + 0.4 * 0.75)).abs() < 1e-3);
        run(&mut e, &ep, 0.2, 0.001);
        assert_eq!((e.stage(), e.value()), (Stage::Sustain, 0.4));
        // release 300 ms from 0.4: halfway f = 0.25
        e.release();
        assert!((run(&mut e, &ep, 0.15, 0.001) - 0.4 * 0.25).abs() < 1e-3);
        run(&mut e, &ep, 0.16, 0.001);
        assert!(e.finished() && e.value() == 0.0);
    }

    #[test]
    fn ws_release_starts_from_the_current_value_in_any_stage() {
        let ep = ep_ms(100.0, 200.0, 0.4, 100.0);
        let mut e = Env::new();
        e.trigger(false);
        run(&mut e, &ep, 0.05, 0.001); // mid attack, 0.5
        e.release();
        assert_eq!(e.stage(), Stage::Release);
        assert!((run(&mut e, &ep, 0.05, 0.001) - 0.5 * 0.25).abs() < 1e-3);
        e.release(); // a second release changes nothing
        assert!((e.value() - 0.125).abs() < 1e-3);
    }

    #[test]
    fn step_size_does_not_change_the_ws_result() {
        let ep = ep_ms(30.0, 80.0, 0.3, 120.0);
        let at = |step: f32| {
            let mut e = Env::new();
            e.trigger(false);
            let a = run(&mut e, &ep, 0.055, step);
            e.release();
            (a, run(&mut e, &ep, 0.06, step))
        };
        let (a, b) = at(0.0001);
        for step in [0.0005, 0.001, 0.005] {
            let (x, y) = at(step);
            assert!((x - a).abs() < 2e-3 && (y - b).abs() < 2e-3, "step {step}: {x} {y} vs {a} {b}");
        }
    }

    fn tf(a: f32, d: f32, s: f32, r: f32, slope: f32) -> EnvP {
        EnvP { law: time_law::RATE, curve: curve::POWER, a, d, s, r, curve_slope: slope, ..EnvP::DEFAULT }
    }

    /// The original per-block recurrence, step 1 block, as an independent model.
    fn tf_model(ep: &EnvP, blocks: usize, release_at: usize) -> Vec<f32> {
        let step = 0.0005 * 512.0;
        let cube = |x: f32| x * x * x;
        let (mut v, mut stage) = (0.0f32, 0u8); // 0 attack, 1 decay, 2 sustain, 3 release, 4 done
        let s = ep.s.min(0.99);
        let mut out = vec![];
        for b in 0..blocks {
            if b == release_at && stage < 3 {
                stage = 3;
            }
            match stage {
                0 => {
                    v += -(cube(ep.a).max(1e-9) * 0.94).log10() * step;
                    if v >= 1.0 {
                        v = 1.0;
                        stage = 1;
                    }
                }
                1 => {
                    if v < s {
                        v = s;
                        stage = 2;
                    } else {
                        let pos = (0.01 + (v - s)) / (1.0 - s);
                        v += (cube(ep.d).max(1e-9) * 0.94).log10() * 0.25 * step * pos.powf(ep.curve_slope);
                        if v <= s {
                            v = s;
                            stage = 2;
                        }
                    }
                }
                2 => v = s,
                3 => {
                    v += (cube(ep.r).max(1e-9) * 0.94).log10() * 0.25 * step * v.powf(ep.curve_slope);
                    if v <= 1e-5 {
                        v = 0.0;
                        stage = 4;
                    }
                }
                _ => {}
            }
            out.push(v);
        }
        out
    }

    #[test]
    fn tf_curves_follow_the_original_recurrence_in_time() {
        let ep = tf(0.6, 0.35, 0.45, 0.4, 0.5);
        let model = tf_model(&ep, 400, 200);
        let mut e = Env::new();
        e.trigger(false);
        let block = 512.0 / 44_100.0;
        let mut worst = 0.0f32;
        for b in 0..400 {
            if b == 200 {
                e.release();
            }
            // the model's value after block b+1 is the state at time (b+1)·block
            let step = 32.0 / 44_100.0;
            for _ in 0..16 {
                e.advance(step, &ep, 1.0);
            }
            worst = worst.max((e.value() - model[b]).abs());
            let _ = block;
        }
        assert!(worst < 0.05, "{worst}: Euler at 32 frames vs the original 512-frame recurrence");
        assert!(model[199] > 0.4 && model[199] < 0.6, "model sits near sustain before the release");
    }

    #[test]
    fn tf_attack_decay_release_reach_their_targets() {
        let ep = tf(0.5, 0.5, 0.3, 0.5, 0.5);
        let mut e = Env::new();
        e.trigger(false);
        run(&mut e, &ep, 0.3, 32.0 / 44_100.0);
        assert!(matches!(e.stage(), Stage::Decay | Stage::Sustain));
        run(&mut e, &ep, 3.0, 32.0 / 44_100.0);
        assert_eq!((e.stage(), e.value()), (Stage::Sustain, 0.3));
        e.release();
        run(&mut e, &ep, 5.0, 32.0 / 44_100.0);
        assert!(e.finished());
    }

    #[test]
    fn tf_decay_that_would_rise_holds_at_the_peak() {
        let ep = tf(0.0, 1.0, 0.5, 0.5, 0.5);
        let mut e = Env::new();
        e.trigger(false);
        for _ in 0..2000 {
            e.advance(32.0 / 44_100.0, &ep, 1.1); // d·M = 1.1 → log10(0.94·1.33) > 0
        }
        assert!(e.value() <= 1.0 && e.value() > 0.99, "{}", e.value());
        assert_eq!(e.stage(), Stage::Decay);
        // and with M = 1 it decays normally
        let mut e = Env::new();
        e.trigger(false);
        for _ in 0..2000 {
            e.advance(32.0 / 44_100.0, &ep, 1.0);
        }
        assert!(e.value() < 1.0);
    }

    #[test]
    fn unused_env_is_finished_and_silent() {
        let mut e = Env::new();
        e.advance(1.0, &WS, 1.0);
        assert!(e.finished() && e.value() == 0.0);
        e.release();
        assert!(e.finished());
    }
}

/// A patch that asks for a mode the build lacks fails at load; modes of envelopes nobody reads do not count.
#[cfg(all(test, feature = "us-env-seconds"))]
mod gating {
    use super::*;
    use crate::chunk::encode;
    use crate::instrument::UInstrument;
    #[cfg(feature = "us-matrix")]
    use crate::law::host as inv;
    use crate::module;
    #[cfg(feature = "us-matrix")]
    use crate::modmx::{source, target};
    use synth_core::mix::LoadError;

    fn need(edits: &[(usize, f32)]) -> Result<usize, LoadError> {
        let mut v = pm::DEFAULTS;
        edits.iter().for_each(|&(i, x)| v[i] = x);
        UInstrument::need(&encode(&v, &[]))
    }

    fn expect_rate(r: Result<usize, LoadError>) {
        if cfg!(feature = "us-env-rate") {
            assert!(r.is_ok());
        } else {
            assert_eq!(r, Err(LoadError::UnsupportedModule(module::ENV_RATE)));
        }
    }

    #[test]
    fn the_amplitude_envelope_always_counts() {
        expect_rate(need(&[(pm::env(0, e::LAW), pm::env_law::encode(pm::env_law::RATE))]));
    }

    #[cfg(feature = "us-matrix")]
    #[test]
    fn another_envelope_counts_only_when_the_matrix_reads_it() {
        assert!(need(&[(pm::env(2, e::LAW), pm::env_law::encode(pm::env_law::RATE))]).is_ok(), "not read: its mode is irrelevant");
        let read = [
            (pm::env(2, e::LAW), pm::env_law::encode(pm::env_law::RATE)),
            (pm::modslot(5, pm::m::SOURCE), inv::inv_choice(source::ENV1 + 2, source::COUNT)),
            (pm::modslot(5, pm::m::TARGET), inv::inv_index(target::GAIN)),
        ];
        expect_rate(need(&read));
    }

    #[test]
    fn an_automated_mode_the_build_lacks_runs_the_mode_it_has() {
        // with the timed mode built in, the rate mode is only reachable when it is built in too
        assert!(!of_mode(false));
        assert_eq!(of_mode(true), cfg!(feature = "us-env-rate"));
    }
}
