//! Shared lookup tables, computed once at first use from a fixed seed (never embedded in the binary): a 4096-point
//! sine (one period), the generator's exp(−x) profile over [0, 5), the per-bin random vector for the phase stage,
//! the stepped-random LFO values, the 64K looped white-noise table and the 12-TET note frequencies.
//! Read-only after initialisation, so instruments share them without global mutable state.

use crate::rng::Xorshift;
use core::sync::atomic::{AtomicU8, Ordering};
use synth_core::rng::Rng;

/// Generator spectrum bins (complex) = wavetable length.
pub const BINS: usize = 512;
/// Sine / exp table length.
pub const TAB: usize = 4096;
pub const NOISE_LEN: usize = 65536;
pub const LFO_NOISE_LEN: usize = 256;
/// Seed of the table generator (independent of the voices' random source).
const TABLE_SEED: u32 = 0x5EED_7AB1;

pub struct Tables {
    /// `sin(2π·i/4096)`.
    pub sin: [f32; TAB],
    /// `exp(−5i/4096)`: the partial profile, indexed by `trunc(d/5 · 4095)` for a distance d < 5 widths.
    pub exp: [f32; TAB],
    /// `sin(uniform[0, 2π))` per spectrum bin (phase-stage randomisation).
    pub rho: [f32; BINS],
    /// Uniform [0, 1) steps of the random LFO shape.
    pub lfo_noise: [f32; LFO_NOISE_LEN],
    /// Looped white noise, uniform in about [−1, 1].
    pub white: [f32; NOISE_LEN],
    /// MIDI note → Hz (A4 = 440, built by repeated semitone steps in f64 from note 0).
    pub note_hz: [f32; 128],
}

static mut TABLES: Tables = Tables {
    sin: [0.0; TAB],
    exp: [0.0; TAB],
    rho: [0.0; BINS],
    lfo_noise: [0.0; LFO_NOISE_LEN],
    white: [0.0; NOISE_LEN],
    note_hz: [0.0; 128],
};
/// 0 = empty, 1 = being built, 2 = ready.
static STATE: AtomicU8 = AtomicU8::new(0);

/// Semitone ratio as the engine's f32 constant.
pub const SEMITONE: f32 = 1.059_463_1;

fn build(t: &mut Tables) {
    let mut r = Xorshift::with_seed(TABLE_SEED);
    let two_pi = 2.0 * core::f32::consts::PI;
    for i in 0..TAB {
        t.sin[i] = libm::sinf(i as f32 / TAB as f32 * 2.0 * core::f32::consts::PI);
        t.exp[i] = libm::expf(-(5.0 / TAB as f32 * i as f32));
    }
    for v in t.rho.iter_mut() {
        *v = libm::sinf(r.range(0.0, two_pi));
    }
    for v in t.lfo_noise.iter_mut() {
        *v = r.next_f32();
    }
    // three equal "uniform" terms (the reference's degenerate triangular law): uniform over ±32766/32767
    let c1 = 32767.0f32;
    let c2 = (c1 / 3.0) as i32 as f32 + 1.0;
    let c3 = 1.0 / c1;
    for v in t.white.iter_mut() {
        let u = r.next_f32();
        *v = (2.0 * ((u * c2) + (u * c2) + (u * c2)) - 3.0 * (c2 - 1.0)) * c3;
    }
    let step = SEMITONE as f64;
    let mut hz = 6.875 * step * step * step; // C-1 from the A below
    for v in t.note_hz.iter_mut() {
        *v = hz as f32;
        hz *= step;
    }
}

/// The shared tables (built on first call; concurrent first calls wait for one builder).
pub fn tables() -> &'static Tables {
    if STATE.load(Ordering::Acquire) != 2 {
        if STATE.compare_exchange(0, 1, Ordering::AcqRel, Ordering::Acquire).is_ok() {
            // SAFETY: only the thread that won 0 → 1 writes; readers wait for state 2.
            build(unsafe { &mut *core::ptr::addr_of_mut!(TABLES) });
            STATE.store(2, Ordering::Release);
        } else {
            while STATE.load(Ordering::Acquire) != 2 {
                core::hint::spin_loop();
            }
        }
    }
    // SAFETY: read-only after publication (state 2, Release/Acquire).
    unsafe { &*core::ptr::addr_of!(TABLES) }
}
