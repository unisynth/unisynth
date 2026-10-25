//! Port of upstream `Helpers`: WaveSabre-specific numerics (FastSin table, random law, parameter curves).
//! `exp2`/`pow` go through `libm::pow(2, x)`, the upstream non-MSVC path (the x87 paths are not reproduced).

use core::f64::consts::PI;
use core::sync::atomic::{AtomicU8, Ordering};
use synth_core::rng::Rng;

// ---- FastSin table ----

const TAB_LOG2: u32 = 9;
const TAB_SIZE: usize = 1 << TAB_LOG2;

/// `sin(i · 2π/512)`, i in 0..=512. Written once by [`init`], read-only afterwards.
static mut TAB: [f64; TAB_SIZE + 1] = [0.0; TAB_SIZE + 1];
/// 0 = empty, 1 = being written, 2 = ready.
static TAB_STATE: AtomicU8 = AtomicU8::new(0);

/// Builds the sine table (idempotent; callers race safely). Every device constructor calls it.
pub fn init() {
    if TAB_STATE.load(Ordering::Acquire) == 2 {
        return;
    }
    if TAB_STATE.compare_exchange(0, 1, Ordering::AcqRel, Ordering::Acquire).is_ok() {
        let step = (PI * 2.0) / TAB_SIZE as f64;
        for i in 0..=TAB_SIZE {
            // SAFETY: only the thread that won the 0 → 1 transition writes, and readers wait for state 2.
            unsafe { (*core::ptr::addr_of_mut!(TAB))[i] = libm::sin(i as f64 * step) };
        }
        TAB_STATE.store(2, Ordering::Release);
    } else {
        while TAB_STATE.load(Ordering::Acquire) != 2 {
            core::hint::spin_loop();
        }
    }
}

#[inline]
fn tab() -> &'static [f64; TAB_SIZE + 1] {
    debug_assert_eq!(TAB_STATE.load(Ordering::Relaxed), 2, "helpers::init not called");
    // SAFETY: read-only after `init` published it (state 2, Release/Acquire).
    unsafe { &*core::ptr::addr_of!(TAB) }
}

/// Upstream `FastSin`: phase → 1 + frac in [1, 2), whose top 32 mantissa bits give a 9-bit table index and a
/// 23-bit interpolation fraction.
#[inline]
pub fn fast_sin(x: f64) -> f64 {
    let x = x * (1.0 / (PI * 2.0));
    let phase = x - libm::floor(x) + 1.0;
    let significand = (phase.to_bits() >> (52 - 32)) as u32;
    const FRACT_BITS: u32 = 32 - TAB_LOG2;
    let index = (significand >> FRACT_BITS) as usize;
    let fract = (significand & ((1 << FRACT_BITS) - 1)) as i32;
    let t = tab();
    let (left, right) = (t[index], t[index + 1]);
    left + (right - left) * (fract as f64 * (1.0 / (1u32 << FRACT_BITS) as f64))
}

#[inline]
pub fn fast_cos(x: f64) -> f64 {
    fast_sin(x + core::f64::consts::FRAC_PI_2)
}

pub fn square135(phase: f64) -> f64 {
    fast_sin(phase) + fast_sin(phase * 3.0) / 3.0 + fast_sin(phase * 5.0) / 5.0
}

pub fn square35(phase: f64) -> f64 {
    fast_sin(phase * 3.0) / 3.0 + fast_sin(phase * 5.0) / 5.0
}

// ---- random law ----

/// Upstream `RandFloat`: `seed *= 0x15A4E35` on a signed 32-bit int, `(seed % 255) / 255` with truncating remainder
/// (range ≈ [−1, 1]). Seed 1 at song start. One per song, drawn in track render order.
#[derive(Clone, Debug)]
pub struct WsRng(i32);

impl WsRng {
    pub fn new() -> Self {
        WsRng(1)
    }
}

impl Default for WsRng {
    fn default() -> Self {
        Self::new()
    }
}

impl Rng for WsRng {
    #[inline]
    fn next_f32(&mut self) -> f32 {
        self.0 = self.0.wrapping_mul(0x15a4e35);
        (self.0 % 255) as f32 / 255.0
    }
}

// ---- exponentials ----

pub fn exp2(x: f64) -> f64 {
    libm::pow(2.0, x)
}

pub fn exp2f(x: f32) -> f32 {
    libm::powf(2.0, x)
}

pub fn exp10f(x: f32) -> f32 {
    exp2f(x * (core::f64::consts::LN_10 / core::f64::consts::LN_2) as f32)
}

pub fn pow2(x: f64) -> f64 {
    x * x
}

pub fn pow4(x: f64) -> f64 {
    pow2(pow2(x))
}

pub fn pow2f(x: f32) -> f32 {
    x * x
}

pub fn note_to_freq(note: f64) -> f64 {
    440.0 * exp2((note - 69.0) / 12.0)
}

// ---- parameter curves ----

/// Upstream `DelayBuffer`/`ResampleBuffer` length: `(int)((double)ms · rate / 1000)`, at least 1.
pub fn ms_to_len(ms: f32, rate: f64) -> usize {
    ((ms as f64 * rate / 1000.0) as i64).max(1) as usize
}

/// Upstream dB law: `2^(dB/6)` (not `10^(dB/20)`).
pub fn db_to_scalar(db: f32) -> f32 {
    exp2f(db / 6.0)
}

pub fn env_value_to_scalar(value: f32) -> f32 {
    libm::sqrtf((value - 1.0) / 5000.0)
}

/// Envelope time in ms from a 0..1 parameter.
pub fn scalar_to_env_value(scalar: f32) -> f32 {
    scalar * scalar * 5000.0 + 1.0
}

pub fn volume_to_scalar(volume: f32) -> f32 {
    let v = volume * 0.4;
    v * v
}

pub fn param_to_boolean(value: f32) -> bool {
    value >= 0.5
}

pub fn param_to_frequency(param: f32) -> f32 {
    20.0 + (20000.0 - 20.0) * param * param
}

pub fn param_to_q(param: f32) -> f32 {
    if param < 0.5 { param / 0.5 * (1.0 - 0.33) + 0.33 } else { (param - 0.5) / 0.5 * 11.0 + 1.0 }
}

/// Upstream default range is 24 dB.
pub fn param_to_db(param: f32, range: f32) -> f32 {
    (param * 2.0 - 1.0) * range
}

pub fn param_to_resonance(param: f32) -> f32 {
    param * 0.99 + 0.01
}

pub fn param_to_unisono(param: f32) -> u32 {
    (param * 15.0) as u32 + 1
}

pub fn param_to_vibrato_freq(param: f32) -> f64 {
    (pow2(param as f64) + 0.1) * 70.0
}

pub fn pan_to_scalar_left(pan: f32) -> f32 {
    libm::sqrtf(1.0 - pan)
}

pub fn pan_to_scalar_right(pan: f32) -> f32 {
    libm::sqrtf(pan)
}
