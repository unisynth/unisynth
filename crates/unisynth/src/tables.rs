//! Shared read-only tables, built once on first use: a 4096-point sine (one period) read with linear interpolation,
//! and, with the wavetable oscillator, its `exp(−x)` partial profile and per-bin random phase vector.
//! The only global state; it is immutable after `init` and identical on every target (built with `libm`).

use core::sync::atomic::{AtomicU8, Ordering};

const SIZE: usize = 4096;

/// `sin(2π·i/4096)`, `i` in `0..=4096` (the last entry closes the period for interpolation).
static mut SIN: [f32; SIZE + 1] = [0.0; SIZE + 1];
/// Wavetable partial profile `exp(−5i/4096)`.
#[cfg(feature = "us-osc-wt")]
static mut EXP: [f32; SIZE] = [0.0; SIZE];
/// Wavetable phase stage: `sin(uniform[0, 2π))` per spectrum bin (the original generator's sequence).
#[cfg(feature = "us-osc-wt")]
static mut RHO: [f32; 512] = [0.0; 512];
/// Seed of the table generator (independent of every voice's random source); the original value.
#[cfg(feature = "us-osc-wt")]
const TABLE_SEED: u32 = 0x5EED_7AB1;
/// 0 = empty, 1 = being written, 2 = ready.
static STATE: AtomicU8 = AtomicU8::new(0);

/// Builds the tables (idempotent; callers race safely). Every instrument constructor calls it.
pub fn init() {
    if STATE.load(Ordering::Acquire) == 2 {
        return;
    }
    if STATE.compare_exchange(0, 1, Ordering::AcqRel, Ordering::Acquire).is_ok() {
        for i in 0..=SIZE {
            let v = crate::fm::sin(i as f64 * (2.0 * core::f64::consts::PI / SIZE as f64)) as f32;
            // SAFETY: only the thread that won the 0 → 1 transition writes; readers wait for state 2.
            unsafe { (*core::ptr::addr_of_mut!(SIN))[i] = v };
        }
        #[cfg(feature = "us-osc-wt")]
        {
            let mut r = crate::rng::Xorshift::new(TABLE_SEED);
            for i in 0..SIZE {
                // SAFETY: as above, only the initialising thread writes.
                unsafe { (*core::ptr::addr_of_mut!(EXP))[i] = crate::fm::expf(-(5.0 / SIZE as f32 * i as f32)) };
            }
            for i in 0..512 {
                use synth_core::rng::Rng;
                let v = crate::fm::sinf(r.next_f32() * (2.0 * core::f32::consts::PI));
                // SAFETY: as above.
                unsafe { (*core::ptr::addr_of_mut!(RHO))[i] = v };
            }
        }
        STATE.store(2, Ordering::Release);
    } else {
        while STATE.load(Ordering::Acquire) != 2 {
            core::hint::spin_loop();
        }
    }
}

#[inline]
fn sin_table() -> &'static [f32; SIZE + 1] {
    debug_assert_eq!(STATE.load(Ordering::Relaxed), 2, "tables::init not called");
    // SAFETY: read-only after `init` published it (state 2, Release/Acquire).
    unsafe { &*core::ptr::addr_of!(SIN) }
}

/// `sin(2π·x)` for a phase `x` in turns (any value; only the fractional part counts), linear interpolation.
#[inline]
pub fn sin_turns(x: f64) -> f32 {
    let t = sin_table();
    let f = x - libm::floor(x);
    let pos = f * SIZE as f64;
    let i = pos as usize; // 0..=4095 (f < 1)
    let frac = (pos - i as f64) as f32;
    let a = t[i];
    a + (t[i + 1] - a) * frac
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sine_matches_libm_closely_and_wraps() {
        init();
        let mut worst = 0.0f64;
        for k in 0..20_000 {
            let x = k as f64 / 20_000.0 * 3.0 - 1.0; // −1 .. 2 turns
            let want = libm::sin(x * 2.0 * core::f64::consts::PI);
            worst = worst.max((sin_turns(x) as f64 - want).abs());
        }
        assert!(worst < 4e-7, "{worst}"); // linear interpolation: (2π/4096)²/8 ≈ 2.9e-7
        assert_eq!(sin_turns(0.0), 0.0);
        assert!((sin_turns(0.25) - 1.0).abs() < 1e-6);
        assert!((sin_turns(1.25) - sin_turns(0.25)).abs() < 1e-6);
        assert!((sin_turns(-0.75) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn init_is_idempotent() {
        init();
        let a = sin_turns(0.123);
        init();
        assert_eq!(a, sin_turns(0.123));
    }
}

/// `sin(2π·i/4096)` for a table index `i` in `0..4096`.
#[inline]
pub fn sin_at(i: usize) -> f32 {
    sin_table()[i & (SIZE - 1)]
}

/// The wavetable partial profile: `exp(−5i/4096)`.
#[cfg(feature = "us-osc-wt")]
#[inline]
pub fn exp_profile() -> &'static [f32; SIZE] {
    // SAFETY: read-only after `init` published it.
    unsafe { &*core::ptr::addr_of!(EXP) }
}

/// The wavetable phase-stage random vector.
#[cfg(feature = "us-osc-wt")]
#[inline]
pub fn rho() -> &'static [f32; 512] {
    // SAFETY: read-only after `init` published it.
    unsafe { &*core::ptr::addr_of!(RHO) }
}
