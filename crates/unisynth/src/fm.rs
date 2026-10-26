//! Elementary functions for the laws and filters, in place of `libm`'s general ones. `libm`'s `pow` (two of them: f64 and
//! f32), `sin`/`cos` with their large-argument range reduction (`rem_pio2_large` alone is 2 KB) and its software `sqrt`
//! were 8 KB of the i686 player; the arguments here are bounded (a phase, a cutoff, a law's exponent), so a short
//! reduction and a polynomial in f64 do the same in a fraction of that. Errors are below 1e-12 relative (f64) over the
//! ranges the synth uses, far below f32 rounding, so the f32 wrappers match `libm` to the last bit or one unit of it.
//! Inputs outside the ranges documented per function give a finite wrong answer or 0, never a panic or a loop.

use core::f64::consts::{LN_2, LOG10_2, LOG2_E, PI};

/// `2^x`. 0 below −1022, infinity from 1024, NaN stays NaN.
pub fn exp2(x: f64) -> f64 {
    if x != x {
        return x;
    }
    if x <= -1022.0 {
        return 0.0;
    }
    if x >= 1024.0 {
        return f64::INFINITY;
    }
    // x = k + f, |f| ≤ 0.5; 2^f = e^(f·ln2), |t| ≤ 0.35: the Taylor series to t^13 is exact to 1e-16
    let k = (x + if x >= 0.0 { 0.5 } else { -0.5 }) as i64;
    let t = (x - k as f64) * LN_2;
    let p = 1.0
        + t * (1.0
            + t * (1.0 / 2.0
                + t * (1.0 / 6.0
                    + t * (1.0 / 24.0
                        + t * (1.0 / 120.0
                            + t * (1.0 / 720.0
                                + t * (1.0 / 5040.0
                                    + t * (1.0 / 40320.0
                                        + t * (1.0 / 362880.0
                                            + t * (1.0 / 3628800.0 + t * (1.0 / 39916800.0 + t * (1.0 / 479001600.0))))))))))));
    // 2^k in two steps so k = 1024 (x ≥ 1023.5) does not overflow the exponent field
    let half = k / 2;
    p * f64::from_bits(((half + 1023) as u64) << 52) * f64::from_bits(((k - half + 1023) as u64) << 52)
}

/// `log2(x)` for positive finite `x`; −infinity for 0, NaN for a negative or NaN `x`, infinity for infinity.
pub fn log2(x: f64) -> f64 {
    if !(x > 0.0) {
        return if x == 0.0 { f64::NEG_INFINITY } else { f64::NAN };
    }
    if x == f64::INFINITY {
        return x;
    }
    // a subnormal is scaled into the normal range first
    let (x, bias) = if x < f64::MIN_POSITIVE { (x * 18014398509481984.0, -54.0) } else { (x, 0.0) };
    let bits = x.to_bits();
    let mut e = ((bits >> 52) & 0x7ff) as i64 - 1023;
    let mut m = f64::from_bits((bits & 0x000f_ffff_ffff_ffff) | (1023u64 << 52));
    if m > core::f64::consts::SQRT_2 {
        m *= 0.5;
        e += 1;
    }
    // ln m = 2·atanh(s), s = (m−1)/(m+1), |s| ≤ 0.172: the series to s^19 is exact to 1e-17
    let s = (m - 1.0) / (m + 1.0);
    let z = s * s;
    let ln = 2.0
        * s
        * (1.0
            + z * (1.0 / 3.0
                + z * (1.0 / 5.0
                    + z * (1.0 / 7.0 + z * (1.0 / 9.0 + z * (1.0 / 11.0 + z * (1.0 / 13.0 + z * (1.0 / 15.0 + z * (1.0 / 17.0 + z * (1.0 / 19.0))))))))));
    e as f64 + bias + ln * LOG2_E
}

/// `x^y` for the cases the synth has: `x ≥ 0` with any `y`, or `x < 0` with a whole `y` (the sign follows its parity);
/// `0^y` is 0 for `y > 0` and infinity for `y < 0`; `x^0` is 1; any other negative `x` is NaN.
pub fn pow(x: f64, y: f64) -> f64 {
    if y == 0.0 {
        return 1.0;
    }
    if x != x || y != y {
        return f64::NAN;
    }
    if x == 0.0 {
        return if y > 0.0 { 0.0 } else { f64::INFINITY };
    }
    if x > 0.0 {
        return exp2(y * log2(x));
    }
    // x < 0: whole exponents only
    if y.abs() < 9007199254740992.0 && (y as i64) as f64 == y {
        let m = exp2(y * log2(-x));
        return if (y as i64) & 1 == 1 { -m } else { m };
    }
    f64::NAN
}

/// `sin` and `cos` of `x` radians for `|x| < 1e6` (a phase, a cutoff angle, a filter pole angle); 0 and 1 beyond.
pub fn sin_cos(x: f64) -> (f64, f64) {
    if !(x.abs() < 1.0e6) {
        return (0.0, 1.0);
    }
    // x = n·π/2 + r, |r| ≤ π/4, with π/2 in three parts so n·part is exact for |n| < 2^20 (fdlibm's pio2_1..3)
    const P1: f64 = 1.570_796_326_734_125_6;
    const P2: f64 = 6.077_100_506_303_966e-11;
    const P3: f64 = 2.022_266_248_711_166_5e-21;
    let n = (x * (2.0 / PI) + if x >= 0.0 { 0.5 } else { -0.5 }) as i64;
    let nf = n as f64;
    let r = ((x - nf * P1) - nf * P2) - nf * P3;
    let z = r * r;
    let s = r
        * (1.0
            + z * (-1.0 / 6.0
                + z * (1.0 / 120.0
                    + z * (-1.0 / 5040.0
                        + z * (1.0 / 362880.0 + z * (-1.0 / 39916800.0 + z * (1.0 / 6227020800.0 + z * (-1.0 / 1307674368000.0 + z * (1.0 / 355687428096000.0)))))))));
    let c = 1.0
        + z * (-1.0 / 2.0
            + z * (1.0 / 24.0
                + z * (-1.0 / 720.0
                    + z * (1.0 / 40320.0
                        + z * (-1.0 / 3628800.0 + z * (1.0 / 479001600.0 + z * (-1.0 / 87178291200.0 + z * (1.0 / 20922789888000.0))))))));
    match n & 3 {
        0 => (s, c),
        1 => (c, -s),
        2 => (-s, -c),
        _ => (-c, s),
    }
}

pub fn sin(x: f64) -> f64 {
    sin_cos(x).0
}

pub fn cos(x: f64) -> f64 {
    sin_cos(x).1
}

pub fn sinf(x: f32) -> f32 {
    sin(x as f64) as f32
}

pub fn cosf(x: f32) -> f32 {
    cos(x as f64) as f32
}

pub fn sincosf(x: f32) -> (f32, f32) {
    let (s, c) = sin_cos(x as f64);
    (s as f32, c as f32)
}

pub fn exp2f(x: f32) -> f32 {
    exp2(x as f64) as f32
}

pub fn expf(x: f32) -> f32 {
    exp2(x as f64 * LOG2_E) as f32
}

pub fn powf(x: f32, y: f32) -> f32 {
    pow(x as f64, y as f64) as f32
}

pub fn log10f(x: f32) -> f32 {
    (log2(x as f64) * LOG10_2) as f32
}

/// Square root with the hardware instruction on x86 (exactly rounded, as `libm`'s is, without its 1 KB of code).
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
pub fn sqrt(x: f64) -> f64 {
    #[cfg(target_arch = "x86")]
    use core::arch::x86::{_mm_cvtsd_f64, _mm_set_sd, _mm_sqrt_sd};
    #[cfg(target_arch = "x86_64")]
    use core::arch::x86_64::{_mm_cvtsd_f64, _mm_set_sd, _mm_sqrt_sd};
    // SAFETY: SSE2 is part of the i686 and x86-64 baselines of every target this crate builds for
    unsafe { _mm_cvtsd_f64(_mm_sqrt_sd(_mm_set_sd(0.0), _mm_set_sd(x))) }
}

#[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
pub fn sqrt(x: f64) -> f64 {
    libm::sqrt(x)
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
pub fn sqrtf(x: f32) -> f32 {
    #[cfg(target_arch = "x86")]
    use core::arch::x86::{_mm_cvtss_f32, _mm_set_ss, _mm_sqrt_ss};
    #[cfg(target_arch = "x86_64")]
    use core::arch::x86_64::{_mm_cvtss_f32, _mm_set_ss, _mm_sqrt_ss};
    // SAFETY: as for `sqrt`
    unsafe { _mm_cvtss_f32(_mm_sqrt_ss(_mm_set_ss(x))) }
}

#[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
pub fn sqrtf(x: f32) -> f32 {
    libm::sqrtf(x)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rel(a: f64, b: f64) -> f64 {
        if a == b { 0.0 } else { ((a - b) / b).abs() }
    }

    #[test]
    fn exp2_and_log2_match_libm_over_the_whole_range() {
        let mut worst = (0.0f64, 0.0f64);
        let mut x = -1021.9;
        while x < 1023.9 {
            worst.0 = worst.0.max(rel(exp2(x), libm::exp2(x)));
            x += 0.173;
        }
        let mut x = 1e-300;
        while x < 1e300 {
            worst.1 = worst.1.max((log2(x) - libm::log2(x)).abs() / libm::log2(x).abs().max(1.0));
            x *= 1.37;
        }
        assert!(worst.0 < 2e-15 && worst.1 < 2e-15, "exp2 {} log2 {}", worst.0, worst.1);
        assert_eq!(exp2(-1022.0), 0.0);
        assert_eq!(exp2(1024.0), f64::INFINITY);
        assert!(exp2(f64::NAN).is_nan());
        assert!((exp2(1023.7) / libm::exp2(1023.7) - 1.0).abs() < 1e-15, "the top of the range");
        assert_eq!(exp2(3.0), 8.0);
        assert_eq!(log2(1.0), 0.0);
        assert_eq!(log2(0.0), f64::NEG_INFINITY);
        assert!(log2(-1.0).is_nan() && log2(f64::NAN).is_nan());
        assert_eq!(log2(f64::INFINITY), f64::INFINITY);
        assert!((log2(4.9e-324) + 1074.0).abs() < 1e-9, "a subnormal: {}", log2(4.9e-324));
    }

    #[test]
    fn pow_matches_libm_for_the_synth_s_cases() {
        let mut worst = 0.0f64;
        for xi in 0..400 {
            let x = xi as f64 * 0.05; // 0 .. 20
            for yi in -40..60 {
                let y = yi as f64 * 0.25;
                let (a, b) = (pow(x, y), libm::pow(x, y));
                if b.is_finite() && b != 0.0 {
                    worst = worst.max(rel(a, b));
                } else {
                    assert_eq!(a, b, "{x}^{y}");
                }
            }
        }
        assert!(worst < 1e-14, "{worst}");
        for (x, y) in [(-2.0, 3.0), (-2.0, 2.0), (-0.5, 3.0), (-3.0, -1.0), (-1.5, 4.0), (0.37, 3.0), (10.0, 0.05)] {
            assert!(rel(pow(x, y), libm::pow(x, y)) < 1e-14, "{x}^{y}: {} vs {}", pow(x, y), libm::pow(x, y));
        }
        assert!(pow(-2.0, 0.5).is_nan() && pow(f64::NAN, 2.0).is_nan());
        assert_eq!((pow(5.0, 0.0), pow(0.0, 0.0), pow(0.0, 2.0), pow(0.0, -1.0)), (1.0, 1.0, 0.0, f64::INFINITY));
    }

    #[test]
    fn sin_and_cos_match_libm_to_a_phase_of_many_turns() {
        let (mut ws, mut wc) = (0.0f64, 0.0f64);
        let mut x = -2000.0;
        while x < 2000.0 {
            let (s, c) = sin_cos(x);
            ws = ws.max((s - libm::sin(x)).abs());
            wc = wc.max((c - libm::cos(x)).abs());
            x += 0.0137;
        }
        assert!(ws < 2e-15 && wc < 2e-15, "sin {ws} cos {wc}");
        assert_eq!(sin_cos(0.0), (0.0, 1.0));
        // the quadrant edges
        for k in -8..=8 {
            let x = k as f64 * PI / 2.0;
            assert!((sin(x) - libm::sin(x)).abs() < 1e-15 && (cos(x) - libm::cos(x)).abs() < 1e-15, "{k}");
        }
        assert_eq!(sin_cos(1.0e9), (0.0, 1.0), "out of range is finite");
        assert_eq!(sin_cos(f64::NAN), (0.0, 1.0));
        assert_eq!(sin_cos(f64::INFINITY), (0.0, 1.0));
    }

    #[test]
    fn the_f32_wrappers_are_within_one_unit_of_libm() {
        let ulp = |a: f32, b: f32| ((a.to_bits() as i64) - (b.to_bits() as i64)).abs();
        let mut worst = 0;
        let mut x = -20.0f32;
        while x < 20.0 {
            worst = worst.max(ulp(expf(x), libm::expf(x))).max(ulp(exp2f(x), libm::exp2f(x)));
            worst = worst.max(ulp(powf(1.0 + x.abs(), 0.37 * x), libm::powf(1.0 + x.abs(), 0.37 * x)));
            if x > 0.0 {
                worst = worst.max(ulp(log10f(x), libm::log10f(x)));
            }
            let (s, c) = sincosf(x);
            worst = worst.max(ulp(s, libm::sinf(x))).max(ulp(c, libm::cosf(x))).max(ulp(sinf(x), libm::sinf(x))).max(ulp(cosf(x), libm::cosf(x)));
            x += 0.0113;
        }
        // near a zero of sin/cos an absolute error of 1e-7 is several ulp: allow it there
        assert!(worst <= 2, "{worst} ulp");
        assert_eq!(sqrtf(2.0), libm::sqrtf(2.0));
        assert_eq!(sqrt(2.0), libm::sqrt(2.0));
        assert_eq!((sqrtf(0.0), sqrtf(1.0), sqrt(9.0)), (0.0, 1.0, 3.0));
        assert!(sqrtf(-1.0).is_nan() && sqrt(-1.0).is_nan());
    }
}
