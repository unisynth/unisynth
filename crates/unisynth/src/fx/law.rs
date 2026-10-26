//! Laws shared by the effect modes: the WaveSabre helpers (`exp2(dB/6)`, the frequency and Q curves) and Tunefish's
//! EQ gain, kept exactly as the original engines have them so parameters convert by copying. Each mode uses a few;
//! a build with some modes leaves the rest unused.
#![allow(dead_code)]

/// `2^x`.
pub fn exp2f(x: f32) -> f32 {
    crate::fm::powf(2.0, x)
}

/// `10^x`, as WaveSabre computes it (through `2^x`).
pub fn exp10f(x: f32) -> f32 {
    exp2f(x * (core::f64::consts::LN_10 / core::f64::consts::LN_2) as f32)
}

pub fn pow2f(x: f32) -> f32 {
    x * x
}

/// WaveSabre's `FastSin`: a 512-entry sine table read with a 23-bit linear interpolation fraction. Its error
/// (up to 2·10⁻⁵) shifts low, resonant filter corners noticeably, so the coefficient code keeps it. The two
/// neighbouring entries are computed on the spot (this runs when a setting changes, not per sample).
pub fn fast_sin(x: f64) -> f64 {
    const LOG2: u32 = 9;
    let step = core::f64::consts::PI * 2.0 / (1u32 << LOG2) as f64;
    let x = x * (1.0 / (core::f64::consts::PI * 2.0));
    let phase = x - libm::floor(x) + 1.0;
    let significand = (phase.to_bits() >> (52 - 32)) as u32;
    const FRACT_BITS: u32 = 32 - LOG2;
    let index = (significand >> FRACT_BITS) as usize;
    let fract = (significand & ((1 << FRACT_BITS) - 1)) as i32;
    let (left, right) = (crate::fm::sin(index as f64 * step), crate::fm::sin((index + 1) as f64 * step));
    left + (right - left) * (fract as f64 * (1.0 / (1u32 << FRACT_BITS) as f64))
}

pub fn fast_cos(x: f64) -> f64 {
    fast_sin(x + core::f64::consts::FRAC_PI_2)
}

/// Length in samples of `ms` at `rate`: `(int)(ms · rate / 1000)`, at least 1.
pub fn ms_to_len(ms: f32, rate: f64) -> usize {
    ((ms as f64 * rate / 1000.0) as i64).max(1) as usize
}

/// WaveSabre's dB law: `2^(dB/6)` (not `10^(dB/20)`).
pub fn db_to_scalar(db: f32) -> f32 {
    exp2f(db / 6.0)
}

pub fn param_to_frequency(p: f32) -> f32 {
    20.0 + (20000.0 - 20.0) * p * p
}

pub fn param_to_q(p: f32) -> f32 {
    if p < 0.5 {
        p / 0.5 * (1.0 - 0.33) + 0.33
    } else {
        (p - 0.5) / 0.5 * 11.0 + 1.0
    }
}

/// `±range` dB around 0 at the middle.
pub fn param_to_db(p: f32, range: f32) -> f32 {
    (p * 2.0 - 1.0) * range
}

/// Tunefish 3-band EQ band gain: `2p` up to the middle, then `1 + 10(2p − 1)²`.
pub fn eq_gain(p: f32) -> f32 {
    if p <= 0.5 {
        p * 2.0
    } else {
        let m = (p - 0.5) * 2.0;
        crate::fm::powf(m, 2.0) * 10.0 + 1.0
    }
}

/// WaveSabre's vibrato frequency law (Hz): `(p² + 0.1)·70`.
pub fn param_to_vibrato_freq(p: f32) -> f64 {
    ((p as f64) * (p as f64) + 0.1) * 70.0
}

pub fn param_to_boolean(v: f32) -> bool {
    v >= 0.5
}

/// Envelope-style time in ms from a 0..1 parameter: `p²·5000 + 1`.
pub fn scalar_to_env_value(p: f32) -> f32 {
    p * p * 5000.0 + 1.0
}
