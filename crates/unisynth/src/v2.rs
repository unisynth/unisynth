//! Farbrausch V2's numerics and building blocks (change add-v2-import): the fast sine/arctangent, the frequency laws,
//! fixed-point helpers, the 1× LRC (state-variable) filter of the noise colour, the DC filter and the distortion stage
//! (its filter modes run the shared filter, `flt::run`; change unify-v2-algorithms 2.2). Port of V2
//! by Tammo "kb" Hinrichs (public domain, fr_public/v2), following v2redux's `v2core.cpp` (CC0), which corrects ryg's
//! C++ port to the assembly where they differ (fastatan coefficients and table choice, the all-pass sum order, …).
//! Format-version 6 behaviour; the per-version deltas of v2redux are added only where parity needs them (design D9).
//!
//! V2 works on raw parameter values (`0..128`, `params::v2_raw`) and on 128-sample frames at 44.1 kHz
//! ([`FRAMES_PER_S`] frames per second at any rate).

/// V2 voice state that only a V2 voice feature uses (`us-mono-sum`): stored with it, zero-sized without, so a build
/// without V2 does not carry, copy or initialise it. Without the feature, reads give `T::default()` and writes are
/// dropped (every use sits behind its feature's `cfg!`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Opt<T>(#[cfg(feature = "us-mono-sum")] T, #[cfg(not(feature = "us-mono-sum"))] core::marker::PhantomData<T>);

impl<T: Copy + Default> Opt<T> {
    #[inline(always)]
    pub const fn new(v: T) -> Self {
        #[cfg(feature = "us-mono-sum")]
        return Opt(v);
        #[cfg(not(feature = "us-mono-sum"))]
        {
            let _ = v;
            Opt(core::marker::PhantomData)
        }
    }
    #[inline(always)]
    pub fn set(&mut self, v: T) {
        *self = Opt::new(v);
    }
    #[inline(always)]
    pub fn get(&self) -> T {
        #[cfg(feature = "us-mono-sum")]
        return self.0;
        #[cfg(not(feature = "us-mono-sum"))]
        T::default()
    }
    /// Runs `f` on the value (without the feature: on a default, discarded).
    #[inline(always)]
    pub fn with<R>(&mut self, f: impl FnOnce(&mut T) -> R) -> R {
        #[cfg(feature = "us-mono-sum")]
        return f(&mut self.0);
        #[cfg(not(feature = "us-mono-sum"))]
        f(&mut T::default())
    }
}

/// V2 frames per second: 128 samples at 44.1 kHz; V2 scales its frame to the rate, so this holds at every rate.
pub const FRAMES_PER_S: f32 = 44_100.0 / 128.0;
/// `2^-18`: V2's bias against denormals. Only the DC filter's rounding keeps it (the filter, strip and effect biases
/// were dropped in change unify-v2-algorithms 6.3/6.4; the player flushes denormals).
pub const DC_OFFSET: f32 = 3.814_697_3e-6;
/// Envelopes below `2^-13` (of 0..128) stop.
pub const LOWEST: f32 = 1.220_703_1e-4;
/// Oscillator base frequency (V2's note 60): `261.6255653 / 2`, since the counter wraps at `2^32` but V2 scales by
/// `2^31` (`fcoscbase · fc32bit / sr`).
pub const OSC_BASE_HZ: f32 = 261.625_56 * 0.5;
const PI_2: f32 = core::f32::consts::FRAC_PI_2;
const PI: f32 = core::f32::consts::PI;
const PI_1P5: f32 = 4.712_389;
const TAU: f32 = core::f32::consts::TAU;

/// V2's single-precision minimax sine for `x` in `[−π/2, π/2]`.
#[inline]
pub fn fastsin(x: f32) -> f32 {
    let x2 = x * x;
    (((-0.000_185_42 * x2 + 0.008_314_3) * x2 - 0.166_66) * x2 + 1.0) * x
}

/// V2's sine with range reduction. Kept as the assembly has it: a negative argument is not moved into `[0, 2π)`
/// (only `[−π/2, 0)` then comes out right; FM oscillators pass larger negative values).
#[inline]
pub fn fastsinrc(x: f32) -> f32 {
    let mut x = libm::fmodf(x, TAU);
    if x > PI_1P5 {
        x -= TAU;
    } else if x > PI_2 {
        x = PI - x;
    }
    fastsin(x)
}

/// V2's rational arctangent approximation (assembly coefficients: `cxm2` shared by both tables). Kept as the
/// assembly has it: the `|x| ≥ 1` table is used only for `x` in `[1, 2)`.
#[inline]
pub fn fastatan(x: f32) -> f32 {
    let (sign, x) = if x < 0.0 { (-1.0, -x) } else { (1.0, x) };
    // cx1, cx3, cxm0, cxm2, cxm4, bias
    const C: [[f32; 6]; 2] = [
        [1.0, 0.431_579_74, 1.0, 0.764_439_45, 0.058_319_38, 0.0],
        [-0.431_597_97, -1.0, 0.058_319_38, 0.764_439_45, 1.0, core::f32::consts::FRAC_PI_2],
    ];
    let c = &C[(x >= 1.0 && x < 2.0) as usize];
    let x2 = x * x;
    sign * ((c[1] * x2 + c[0]) * x / ((c[4] * x2 + c[3]) * x2 + c[2]) + c[5])
}

/// `2^((x − 1)·10)`: V2's cutoff/rate law for `x` in `0..1`.
#[inline]
pub fn calcfreq(x: f32) -> f32 {
    crate::fm::exp2f((x - 1.0) * 10.0)
}

/// `2^((x − 1)·11)`: V2's envelope rate law.
#[inline]
pub fn calcfreq2(x: f32) -> f32 {
    crate::fm::exp2f((x - 1.0) * 11.0)
}

/// `x / 2^32` with 23 bits (V2's `utof23`).
#[inline]
pub fn utof23(x: u32) -> f32 {
    f32::from_bits((x >> 9) | 0x3f80_0000) - 1.0
}

/// `v` in `[0, 1)` as 0.32 fixed point, losing a bit as V2 does.
#[inline]
pub fn ftou32(v: f32) -> u32 {
    2u32.wrapping_mul((v * 2_147_483_648.0) as i32 as u32)
}

/// V2's linear congruential generator.
#[inline]
pub fn urandom(seed: &mut u32) -> u32 {
    *seed = seed.wrapping_mul(196_314_165).wrapping_add(907_633_515);
    *seed
}

/// Uniform `[−1, 1)`.
#[inline]
pub fn frandom(seed: &mut u32) -> f32 {
    f32::from_bits((urandom(seed) >> 9) | 0x4000_0000) - 3.0
}

/// V2's sample-rate factor `44100 / rate` (`SRfclinfreq`).
#[inline]
pub fn linfreq(rate: f32) -> f32 {
    44_100.0 / rate
}

/// The LRC (state-variable) filter: `l` low-pass, `b` band-pass state; `step` returns the high-pass.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Lrc {
    pub l: f32,
    pub b: f32,
}

impl Lrc {
    #[inline]
    pub fn step(&mut self, x: f32, f: f32, res: f32) -> f32 {
        self.l += f * self.b;
        let h = x - self.b * res - self.l;
        self.b += f * h;
        h
    }
}

/// The DC filter `y = x − x₋₁ + R·y₋₁` (`V2DCF`), `R = 1 − 126/rate`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Dcf {
    xm1: f32,
    ym1: f32,
}

impl Dcf {
    #[inline]
    pub fn step(&mut self, x: f32, r: f32) -> f32 {
        let y = (DC_OFFSET + r * self.ym1 - self.xm1 + x) - DC_OFFSET;
        self.xm1 = x;
        self.ym1 = y;
        y
    }
}

/// `R` of the DC filter at `rate`.
#[inline]
pub fn dcf_r(rate: f32) -> f32 {
    1.0 - 126.0 / rate
}

/// V2 distortion modes (`V2Dist::Mode`); 5..=11 are the filter modes, `flt::STEPS_KINDS` in order.
pub mod dmode {
    pub const OFF: u8 = 0;
    pub const OVERDRIVE: u8 = 1;
    pub const CLIP: u8 = 2;
    pub const CRUSH: u8 = 3;
    pub const DECIMATE: u8 = 4;
    pub const FLT_BASE: u8 = 4;
}

/// Distortion settings from raw mode, input gain, param 1 and param 2 (`V2Dist::set`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct DistP {
    pub mode: u8,
    gain1: f32,
    gain2: f32,
    offs: f32,
    crush1: f32,
    crush2: i32,
    crxor: i32,
    dfreq: u32,
    /// The filter modes: the shared filter (`flt::run`) on the 7-bit step law.
    fk: crate::flt::Kind,
    flt: [f32; crate::flt::NC],
    /// Format versions below 6: the overdrive uses the exact arctangent (v2redux `DELTA_NATIVE_FPATAN`).
    native: bool,
}

impl DistP {
    pub fn new(mode: f32, ingain: f32, p1: f32, p2: f32, rate: f32) -> Self {
        let mode = (mode as i32).clamp(0, 10) as u8;
        let gain1 = crate::fm::exp2f((ingain - 32.0) / 16.0);
        let mut d = DistP { mode, gain1, ..Default::default() };
        match mode {
            dmode::OFF => {}
            dmode::OVERDRIVE => {
                d.gain2 = (p1 / 128.0) / libm::atanf(gain1);
                d.offs = gain1 * 2.0 * ((p2 / 128.0) - 0.5);
            }
            dmode::CLIP => {
                d.gain2 = p1 / 128.0;
                d.offs = gain1 * 2.0 * ((p2 / 128.0) - 0.5);
            }
            dmode::CRUSH => {
                let x = p1 * 256.0 + 1.0;
                d.crush2 = x as i32;
                d.crush1 = gain1 * (32_768.0 / x);
                d.crxor = (p2 as i32) << 9;
            }
            dmode::DECIMATE => d.dfreq = ftou32(calcfreq(p1 / 128.0)),
            _ => {
                let kind = crate::flt::STEPS_KINDS[(mode - dmode::FLT_BASE - 1) as usize];
                (d.fk, d.flt) = crate::flt::coefs_steps(kind, p1 / 128.0, p2 / 128.0, rate, false);
            }
        }
        d
    }

    /// Overdrive with the exact arctangent (format versions below 6).
    pub fn native_atan(mut self, on: bool) -> Self {
        self.native = on;
        self
    }
}

/// Distortion state: the decimator and one filter per channel.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Dist {
    dcount: u32,
    dval: [f32; 2],
    flt: [[f32; crate::flt::NS]; 2],
}

impl Dist {
    /// One stereo frame (the decimator ticks once per frame).
    #[inline]
    pub fn run(&mut self, p: &DistP, l: f32, r: f32) -> (f32, f32) {
        match p.mode {
            dmode::OFF => (l, r),
            dmode::OVERDRIVE => {
                let at = |x: f32| if p.native { libm::atanf(x) } else { fastatan(x) };
                (p.gain2 * at(l * p.gain1 + p.offs), p.gain2 * at(r * p.gain1 + p.offs))
            }
            dmode::CLIP => (p.gain2 * (l * p.gain1 + p.offs).clamp(-1.0, 1.0), p.gain2 * (r * p.gain1 + p.offs).clamp(-1.0, 1.0)),
            dmode::CRUSH => (crush(p, l), crush(p, r)),
            dmode::DECIMATE => {
                self.dcount = self.dcount.wrapping_add(p.dfreq);
                if self.dcount < p.dfreq {
                    self.dval = [l, r];
                }
                (self.dval[0], self.dval[1])
            }
            _ => (crate::flt::run(p.fk, &p.flt, &mut self.flt[0], l), crate::flt::run(p.fk, &p.flt, &mut self.flt[1], r)),
        }
    }
}

#[inline]
fn crush(p: &DistP, x: f32) -> f32 {
    let t = libm::rintf(x * p.crush1) as i32; // the assembly rounds to nearest (fistp)
    let t = t.wrapping_mul(p.crush2).clamp(-0x7fff, 0x7fff) ^ p.crxor;
    t as f32 / 32_768.0
}

#[cfg(test)]
mod tests;
