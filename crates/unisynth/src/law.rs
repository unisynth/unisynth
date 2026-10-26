//! Parameter laws: normalized `p ∈ [0, 1]` → physical value. Every function clamps `p` (NaN → 0), so any chunk value
//! is safe. Module-specific laws that are copied from an original engine (Falcon ratio, Tunefish `Gen*`, …) live here
//! too, so the converter and the modules share one definition. Design: `add-unified-synth/design.md` D2.
//!
//! The inverses and the [`Law`] table are host-side (tests, converter, editor) and compiled out of the player.

use crate::fm::{powf, sqrtf};
use libm::floorf;

#[inline]
fn c01(p: f32) -> f32 {
    p.max(0.0).min(1.0)
}

/// Filter cutoff in Hz, every filter type: `20 + 19 980·p²` (WaveSabre's law; Tunefish's ladder/RBJ/notch laws are
/// `30 + 20 000p²` / `30 + 10 000p²`, which the converter re-maps through the inverse).
#[inline]
pub fn cutoff(p: f32) -> f32 {
    let p = c01(p);
    20.0 + 19_980.0 * p * p
}

/// Linear gain `16·p²` (0..16, unity at `p = 0.25`). 16 covers Falcon's ×13.25 operator and Tunefish's 0..11 curve.
#[inline]
pub fn gain(p: f32) -> f32 {
    let p = c01(p);
    16.0 * p * p
}

/// LFO rate in Hz, `0.01·10^(4p)` (0.01..100). Covers WaveSabre's vibrato (7..77 Hz) and Tunefish's 0..8 Hz.
#[inline]
pub fn lfo_rate(p: f32) -> f32 {
    0.01 * powf(10.0, 4.0 * c01(p))
}

/// Slide / glide time in seconds, `10·p⁴` (WaveSabre's slide law).
#[inline]
pub fn time(p: f32) -> f32 {
    let p = c01(p);
    let q = p * p;
    10.0 * q * q
}

/// The reach of an oscillator's random start phase, in samples of its own period: `10^(6p − 2)`, 0.01 to 10 000. At
/// `p = 1` it is the whole turn. WaveSabre's oscillators start within a few samples of each other (their random
/// phase is in samples, not turns) and Falcon's within a fraction of one, so unison voices start almost in phase.
#[inline]
pub fn start_spread(p: f32) -> f32 {
    crate::fm::powf(10.0, 6.0 * c01(p) - 2.0)
}

/// The start phase of a slot in turns: `u ∈ [0, 1)` scaled to `start_spread(p)` samples at `inc` turns per sample,
/// at most the whole turn.
#[inline]
pub fn start_phase(u: f64, p: f32, inc: f32) -> f64 {
    if p >= 1.0 { u } else { u * (start_spread(p) as f64 * inc as f64).min(1.0) }
}

/// Voice transpose in semitones, continuous, `±24`.
#[inline]
pub fn transpose(p: f32) -> f32 {
    (c01(p) * 2.0 - 1.0) * 24.0
}

/// Pitch-bend range in semitones, `0..24`.
#[inline]
pub fn bend_range(p: f32) -> f32 {
    c01(p) * 24.0
}

/// Pitch-bend offset in semitones: `bend` 0..1 with its centre 0.5, `range_p` the [`bend_range`] parameter.
#[inline]
pub fn bend_semitones(bend: f32, range_p: f32) -> f32 {
    (c01(bend) - 0.5) * 2.0 * bend_range(range_p)
}

/// Oscillator coarse pitch in whole semitones, `−24..24`.
#[inline]
pub fn coarse(p: f32) -> f32 {
    floorf((c01(p) * 2.0 - 1.0) * 24.0 + 0.5)
}

/// Oscillator fine pitch in semitones, `−1..1`.
#[inline]
pub fn fine(p: f32) -> f32 {
    c01(p) * 2.0 - 1.0
}

/// Oscillator octave shift, whole octaves `−4..4`.
#[inline]
pub fn octave(p: f32) -> f32 {
    floorf((c01(p) - 0.5) * 8.0 + 0.5)
}

/// Frequency ratio, Falcon's coarse law `1 + floor(33p·0.99999)`: 1..33.
#[inline]
pub fn ratio(p: f32) -> f32 {
    1.0 + floorf(c01(p) * 32.99)
}

/// Ratio detune added to [`ratio`], Falcon's fine law `((2p − 1))³` (−1..1).
#[inline]
pub fn ratio_fine(p: f32) -> f32 {
    let f = c01(p) * 2.0 - 1.0;
    f * f * f
}

/// Fixed oscillator frequency in Hz, Tunefish's `1000·p²`.
#[inline]
pub fn fixed_hz(p: f32) -> f32 {
    let p = c01(p);
    1000.0 * p * p
}

/// Feedback (radians per unit of the slot's own output) `8·p²`.
#[inline]
pub fn feedback(p: f32) -> f32 {
    let p = c01(p);
    8.0 * p * p
}

/// Phase-modulation send (radians per unit of the slot's output) `4·p²`.
#[inline]
pub fn pm_out(p: f32) -> f32 {
    let p = c01(p);
    4.0 * p * p
}

/// Equal-power pan: `(left, right) = (√(1 − p), √p)`.
#[inline]
pub fn pan(p: f32) -> (f32, f32) {
    let p = c01(p);
    (sqrtf(1.0 - p), sqrtf(p))
}

/// Envelope time in milliseconds for the timed envelope mode, `p²·5000 + 1`.
#[inline]
pub fn env_ms(p: f32) -> f32 {
    let p = c01(p);
    p * p * 5000.0 + 1.0
}

/// Index `0..n` of an enumeration stored as `i/(n − 1)`.
#[inline]
pub fn choice(p: f32, n: usize) -> usize {
    let top = n.saturating_sub(1);
    // the argument is never negative, where truncation is the floor
    ((c01(p) * top as f32 + 0.5) as usize).min(top)
}

/// Integer in `lo..=hi` stored as `(v − lo)/(hi − lo)`.
#[inline]
pub fn count(p: f32, lo: u32, hi: u32) -> u32 {
    lo + ((c01(p) * (hi - lo) as f32 + 0.5) as u32).min(hi - lo)
}

/// Wavetable unison detune between the left and right phase of a layer, in Hz: `10p³` (Tunefish).
#[inline]
pub fn wt_detune(p: f32) -> f32 {
    let p = c01(p);
    10.0 * p * p * p
}

/// Wavetable unison spread per layer, in Hz: `10p⁴` (Tunefish, which divides it by the rate).
#[inline]
pub fn wt_spread(p: f32) -> f32 {
    let p = c01(p);
    let q = p * p;
    10.0 * q * q
}

/// Wavetable drive before the hard clip: `1 + 32p`.
#[inline]
pub fn wt_drive(p: f32) -> f32 {
    1.0 + 32.0 * c01(p)
}

/// Byte index `0..=255` stored as `i/255` (mod-matrix targets).
#[inline]
pub fn index(p: f32) -> u8 {
    floorf(c01(p) * 255.0 + 0.5) as u8
}

/// Mod amount of a Scale slot, Tunefish's law: `2p` up to 0.5, then `1 + 9(2p − 1)²` (0..10).
#[inline]
pub fn scale_amount(p: f32) -> f32 {
    let p = c01(p);
    if p <= 0.5 {
        p * 2.0
    } else {
        let m = (p - 0.5) * 2.0;
        1.0 + m * m * 9.0
    }
}

/// Mod amount of an Offset slot in the target's physical unit: signed square `sign(x)·x²·range`, `x = 2p − 1`
/// (fine steps near 0 for vibrato depths, ±`range` at the ends).
#[inline]
pub fn offset_amount(p: f32, range: f32) -> f32 {
    let x = c01(p) * 2.0 - 1.0;
    x * libm::fabsf(x) * range
}

/// Host-side: names, laws per index and inverse functions.
#[cfg(any(test, feature = "std"))]
pub mod host {
    use super::*;

    fn inv_sqrt(v: f32, lo: f32, span: f32) -> f32 {
        c01(sqrtf(((v - lo) / span).max(0.0)))
    }

    pub fn inv_start_spread(samples: f32) -> f32 {
        c01((crate::fm::log10f(samples.max(0.01)) + 2.0) / 6.0)
    }
    pub fn inv_cutoff(hz: f32) -> f32 {
        inv_sqrt(hz, 20.0, 19_980.0)
    }
    pub fn inv_gain(g: f32) -> f32 {
        inv_sqrt(g, 0.0, 16.0)
    }
    pub fn inv_lfo_rate(hz: f32) -> f32 {
        c01(crate::fm::log10f((hz / 0.01).max(1.0)) / 4.0)
    }
    pub fn inv_time(s: f32) -> f32 {
        c01(sqrtf(sqrtf((s / 10.0).max(0.0))))
    }
    pub fn inv_bend_range(st: f32) -> f32 {
        c01(st / 24.0)
    }
    pub fn inv_transpose(st: f32) -> f32 {
        c01((st / 24.0 + 1.0) / 2.0)
    }
    /// Coarse semitones (whole numbers) → p.
    pub fn inv_coarse(st: f32) -> f32 {
        c01((st / 24.0 + 1.0) / 2.0)
    }
    pub fn inv_fine(st: f32) -> f32 {
        c01((st + 1.0) / 2.0)
    }
    pub fn inv_octave(oct: f32) -> f32 {
        c01(oct / 8.0 + 0.5)
    }
    pub fn inv_ratio(r: f32) -> f32 {
        c01((r - 1.0) / 32.0)
    }
    pub fn inv_fixed_hz(hz: f32) -> f32 {
        inv_sqrt(hz, 0.0, 1000.0)
    }
    pub fn inv_feedback(f: f32) -> f32 {
        inv_sqrt(f, 0.0, 8.0)
    }
    pub fn inv_pm_out(f: f32) -> f32 {
        inv_sqrt(f, 0.0, 4.0)
    }
    pub fn inv_env_ms(ms: f32) -> f32 {
        inv_sqrt(ms, 1.0, 5000.0)
    }
    pub fn inv_choice(i: usize, n: usize) -> f32 {
        if n < 2 { 0.0 } else { c01(i as f32 / (n - 1) as f32) }
    }
    pub fn inv_count(v: u32, lo: u32, hi: u32) -> f32 {
        if hi <= lo { 0.0 } else { c01((v.saturating_sub(lo)) as f32 / (hi - lo) as f32) }
    }
    pub fn inv_index(i: u8) -> f32 {
        i as f32 / 255.0
    }
    pub fn inv_scale_amount(a: f32) -> f32 {
        let a = a.max(0.0).min(10.0);
        if a <= 1.0 { a / 2.0 } else { 0.5 + sqrtf((a - 1.0) / 9.0) / 2.0 }
    }
    pub fn inv_offset_amount(v: f32, range: f32) -> f32 {
        let r = (v / range).max(-1.0).min(1.0);
        let x = if r < 0.0 { -sqrtf(-r) } else { sqrtf(r) };
        (x + 1.0) / 2.0
    }
}
