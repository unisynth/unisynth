//! The PAD table oscillator (change add-tf3-import, feature `us-osc-pad`; behaviour of Tunefish 3's additive
//! generator, `research/tunefish3-functional-spec.md` §3.2): one static PADsynth table of 2^16 samples per slot and
//! instrument, built once at load from the slot's shape values, played without interpolation, L/R detune, drive and
//! hard clip, through the decimation stage ([`crate::pts::Decim`]).
//!
//! The table is the real part of the inverse DFT of a 2^16-bin spectrum with random phases. It is computed in place in
//! the table itself: the real part of an inverse DFT is the inverse DFT of the spectrum's Hermitian part, whose
//! samples come out of one 2^15-point complex FFT (even samples in the real parts, odd ones in the imaginary parts),
//! so no scratch memory is needed. Bin values are computed when needed and the phases come from a counter hash.

use core::f64::consts::PI;

/// Samples in the table.
pub const LEN: usize = 1 << 16;
/// The original's spectrum resolution: bin `i` is the frequency `i / 2^17` of the build rate.
const BINS: f64 = (1u32 << 17) as f64;
/// The build rate (the table is built for 44.1 kHz at every song rate, so its pitch ratio is fixed).
const RATE: f64 = 44_100.0;
/// The reference frequency the table is built for: MIDI note 29 of the original's tuning table.
pub const F0: f64 = 43.653_528_929_125_486;

/// Profiles (`Choice(4)`).
pub const SINGLE: u8 = 0;
pub const DETUNE: u8 = 1;
pub const SPREAD: u8 = 2;
pub const GAUSS: u8 = 3;

/// The table's inputs (the slot's shape values at load).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Shape {
    pub bandwidth: f32,
    pub scale: f32,
    pub damp: f32,
    pub harmonics: f32,
    pub profile: u8,
}

/// One partial in the original's f32 terms: centre `fi`, width `bwi` (both in units of the build rate) and amplitude.
/// The bin a narrow profile reaches is decided at float precision, so these follow the original's f32 operations.
#[derive(Clone, Copy)]
struct Partial {
    c: f32,
    b: f32,
    a: f32,
    /// The bins its profile can reach (a conservative range; `profile` decides).
    lo: usize,
    hi: usize,
}

/// Half-width of a profile's support, in widths.
fn support(profile: u8) -> f64 {
    match profile {
        SINGLE => 0.1,
        DETUNE => 0.5,
        SPREAD => 5.0,
        _ => libm::sqrt(14.712_806_03),
    }
}

/// The profile's value at offset `d` (f32, frequency units of the build rate) from a partial of width `b`.
fn profile(profile: u8, d: f32, b: f32) -> f32 {
    let x = d / b;
    match profile {
        SINGLE if !(x > 0.1 || x < -0.1) => 1.0 / b,
        DETUNE if !(x < -0.5 || (x > -0.4 && x < 0.4) || x > 0.5) => 1.0 / b,
        SPREAD if !(x > 5.0 || x < -5.0) => 1.0 / b,
        GAUSS if x * x <= 14.712_806 => libm::expf(-(x * x)) / b,
        _ => 0.0,
    }
}

/// The reference note's frequency as the original's tuning table holds it (f64 steps, stored as f32).
fn f0() -> f32 {
    let k = 1.059_463_094_359f64;
    let mut a = 6.875f64 * k * k * k;
    for _ in 0..29 {
        a *= k;
    }
    a as f32
}

/// The partials of `s`, in the original's f32 operation order: `H = 16 + min(round(255·harmonics/4), round(rate/f0))`,
/// partial `h = 1..H` at `f0·h/rate`, width `(2^(256·bw/1200) − 1)·f0·h^(1 + 19·scale²)/(2·rate)`, amplitude
/// `1/(1 + h(1 − damp))` (odd h) or `2/(1 + h(1 − damp))` (even h).
fn partials(s: &Shape, out: &mut [Partial; 96]) -> usize {
    let rint = |x: f32| libm::rintf(x) as usize;
    let (f, rate) = (f0(), RATE as f32);
    let harmonics = s.harmonics / 4.0;
    let h_n = (16 + rint(harmonics * 255.0).min(rint(rate / f))).min(out.len() + 1);
    let bw = s.bandwidth * 256.0;
    let recsr = 1.0f32 / rate;
    let recdsr = recsr / 2.0;
    let precalcbw = (libm::pow(2.0, (bw / 1200.0) as f64) as f32 - 1.0) * f;
    let expo = s.scale * s.scale * 19.0 + 1.0;
    let half = support(s.profile);
    for h in 1..h_n {
        let bw_hz = precalcbw * libm::pow(h as f64, expo as f64) as f32;
        let b = bw_hz * recdsr;
        let c = f * h as f32 * recsr;
        let div = 1.0f32 + h as f32 * (1.0 - s.damp);
        let a = if h % 2 == 1 { 1.0 } else { 2.0 } / div;
        let (lo, hi) = ((c as f64 - half * b as f64) * BINS - 2.0, (c as f64 + half * b as f64) * BINS + 2.0);
        out[h - 1] = Partial { c, b, a, lo: libm::floor(lo.max(0.0)) as usize, hi: (libm::ceil(hi.max(0.0)) as usize).min(LEN - 1) };
    }
    h_n - 1
}

/// Magnitude of bin `i` (`0..LEN`): the profiles summed in partial order in f32, as the original accumulates.
fn amp(parts: &[Partial], profile_id: u8, i: usize) -> f64 {
    let nrec = 1.0f32 / BINS as f32;
    let mut a = 0.0f32;
    for p in parts.iter().filter(|p| i >= p.lo && i <= p.hi) {
        a += profile(profile_id, i as f32 * nrec - p.c, p.b) * p.a;
    }
    a as f64
}

/// Uniform phase in `[0, 2π)` of bin `i` (a SplitMix64 hash of the seed and the bin).
fn phase(seed: u64, i: usize) -> f64 {
    let mut z = seed ^ (i as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^= z >> 31;
    (z >> 11) as f64 / (1u64 << 53) as f64 * 2.0 * PI
}

/// Builds the table of `s` into `t` (`LEN` samples): the real part of the inverse DFT of the random-phase spectrum,
/// scaled by `1/(√2·peak)`.
pub fn build(s: &Shape, seed: u64, t: &mut [f32]) {
    assert_eq!(t.len(), LEN);
    let mut store = [Partial { c: 0.0, b: 1.0, a: 0.0, lo: 1, hi: 0 }; 96];
    let n_parts = partials(s, &mut store);
    let parts = &store[..n_parts];
    let x = |i: usize| -> (f64, f64) {
        let a = amp(parts, s.profile, i % LEN);
        if a == 0.0 {
            return (0.0, 0.0);
        }
        let ph = phase(seed, i % LEN);
        (a * libm::cos(ph), a * libm::sin(ph))
    };
    // the Hermitian part Y[j] = (X[j] + conj(X[−j]))/2
    let y = |j: usize| -> (f64, f64) {
        let (a, b) = (x(j), x((LEN - j) % LEN));
        ((a.0 + b.0) * 0.5, (a.1 - b.1) * 0.5)
    };
    // packed spectrum Z[k] = E[k] + i·O[k]: E = Y[k] + Y[k + N/2], O = (Y[k] − Y[k + N/2])·w^k, w = e^{2πi/N}
    let half = LEN / 2;
    for k in 0..half {
        let (p, q) = (y(k), y(k + half));
        let e = (p.0 + q.0, p.1 + q.1);
        let d = (p.0 - q.0, p.1 - q.1);
        let (sn, cs) = libm::sincos(2.0 * PI * k as f64 / LEN as f64);
        let o = (d.0 * cs - d.1 * sn, d.0 * sn + d.1 * cs);
        // E + i·O
        t[2 * k] = (e.0 - o.1) as f32;
        t[2 * k + 1] = (e.1 + o.0) as f32;
    }
    ifft(t);
    let peak = t.iter().fold(0.0f32, |m, v| m.max(v.abs())).max(1e-5);
    let g = 1.0 / (peak * 1.4142);
    t.iter_mut().for_each(|v| *v *= g);
}

/// In-place radix-2 complex inverse FFT (positive exponent, no scaling) of the interleaved `t` (`t.len()/2` points).
fn ifft(t: &mut [f32]) {
    let n = t.len() / 2;
    let bits = n.trailing_zeros();
    for i in 0..n {
        let j = (i as u32).reverse_bits() as usize >> (32 - bits);
        if i < j {
            t.swap(2 * i, 2 * j);
            t.swap(2 * i + 1, 2 * j + 1);
        }
    }
    let mut m = 2;
    while m <= n {
        let h = m / 2;
        for k in 0..h {
            let (wi, wr) = libm::sincos(2.0 * PI * k as f64 / m as f64);
            let (wr, wi) = (wr as f32, wi as f32);
            let mut a = k;
            while a < n {
                let b = a + h;
                let (br, bi) = (t[2 * b], t[2 * b + 1]);
                let (tr, ti) = (br * wr - bi * wi, br * wi + bi * wr);
                let (ar, ai) = (t[2 * a], t[2 * a + 1]);
                t[2 * b] = ar - tr;
                t[2 * b + 1] = ai - ti;
                t[2 * a] = ar + tr;
                t[2 * a + 1] = ai + ti;
                a += m;
            }
        }
        m *= 2;
    }
}

/// One sample of the table at the oversampled rate: `inc` in periods of the note per sample; the table advances
/// `inc/260` of its length (so the fundamental sounds at about `0.499·f`, an octave below the note), read with the
/// index rounded half to even, driven and hard-clipped.
#[inline]
pub fn sample(t: &[f32], phase: &mut [f32; 2], inc: f32, det: f32, drive: f32) -> (f32, f32) {
    let read = |p: f32| {
        let x = p * LEN as f32;
        let f = x as u32;
        let d = x - f as f32;
        let i = if d > 0.5 || (d == 0.5 && f % 2 == 1) { f + 1 } else { f };
        t[(i as usize) & (LEN - 1)]
    };
    let (l, r) = (read(phase[0]), read(phase[1]));
    for (ph, f) in phase.iter_mut().zip([inc + det, inc - det]) {
        *ph += libm::fabsf(f) / 260.0;
        while *ph > 1.0 {
            *ph -= 1.0;
        }
    }
    ((l * drive).max(-1.0).min(1.0), (r * drive).max(-1.0).min(1.0))
}

/// One output frame through the decimation stage, the slot `gain` before it.
#[inline]
pub fn play(t: &[f32], st: &mut crate::pts::Pts, inc: f32, det: f32, drive: f32, gain: f32) -> (f32, f32) {
    let k = st.decim.factor.max(1);
    let kf = k as f32;
    let (inc, det) = (inc / kf, det / kf);
    let (mut l, mut r) = (0.0f32, 0.0f32);
    for _ in 0..k {
        let (a, b) = sample(t, &mut st.phase[0], inc, det, drive);
        let (a, b) = st.decim.filter(a * gain, b * gain);
        l += a;
        r += b;
    }
    (l / kf, r / kf)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shape(profile: u8, harmonics: f32) -> Shape {
        Shape { bandwidth: 0.3, scale: 0.1, damp: 0.5, harmonics, profile }
    }

    fn naive(s: &Shape, seed: u64, n: usize) -> std::vec::Vec<f64> {
        // the definition directly: Re Σ_k X[k]·e^{2πi·k·m/N}, for the first n samples
        let mut store = [Partial { c: 0.0, b: 1.0, a: 0.0, lo: 1, hi: 0 }; 96];
        let np = partials(s, &mut store);
        let bins: std::vec::Vec<(usize, f64, f64)> =
            (0..LEN).map(|k| (k, amp(&store[..np], s.profile, k), phase(seed, k))).filter(|b| b.1 != 0.0).collect();
        (0..n)
            .map(|m| bins.iter().map(|&(k, a, ph)| a * libm::cos(ph + 2.0 * PI * (k * m % LEN) as f64 / LEN as f64)).sum())
            .collect()
    }

    #[test]
    fn the_packed_transform_is_the_real_part_of_the_inverse_dft() {
        let s = shape(GAUSS, 0.05);
        let mut t = std::vec![0.0f32; LEN];
        build(&s, 7, &mut t);
        let want = naive(&s, 7, 64);
        // compare up to the common scale (the table is peak-normalised)
        let g = want.iter().zip(&t).map(|(w, v)| w * *v as f64).sum::<f64>() / want.iter().map(|w| w * w).sum::<f64>();
        let err = want.iter().zip(&t).map(|(w, v)| (w * g - *v as f64).abs()).fold(0.0f64, f64::max);
        assert!(err < 1e-4, "{err}");
    }

    #[test]
    fn the_table_is_normalised_to_one_over_root_two() {
        let mut t = std::vec![0.0f32; LEN];
        build(&shape(SPREAD, 0.3), 1, &mut t);
        let peak = t.iter().fold(0.0f32, |m, v| m.max(v.abs()));
        assert!((peak - 1.0 / 1.4142).abs() < 1e-5, "{peak}");
    }

    #[test]
    fn partials_follow_the_laws() {
        let mut p = [Partial { c: 0.0, b: 1.0, a: 0.0, lo: 1, hi: 0 }; 96];
        let s = Shape { bandwidth: 0.5, scale: 0.0, damp: 0.0, harmonics: 1.0, profile: GAUSS };
        let n = partials(&s, &mut p);
        assert_eq!(n, 16 + 64 - 1, "H = 16 + round(255/4) = 80: partials 1..80");
        assert!((p[0].c as f64 - F0 / RATE).abs() < 1e-9 && (p[2].c as f64 - 3.0 * F0 / RATE).abs() < 1e-9);
        assert!((p[0].a - 0.5).abs() < 1e-6 && (p[1].a - 2.0 / 3.0).abs() < 1e-6, "odd 1/(1+h), even 2/(1+h)");
        let bw = (libm::pow(2.0, 128.0 / 1200.0) - 1.0) * F0;
        assert!((p[3].b as f64 - bw * 4.0 / (2.0 * RATE)).abs() < 1e-9, "width grows as h at scale 0");
    }

    #[test]
    fn the_fundamental_sounds_an_octave_below_the_note() {
        let mut t = std::vec![0.0f32; LEN];
        build(&Shape { bandwidth: 0.05, scale: 0.0, damp: 0.0, harmonics: 0.0, profile: GAUSS }, 3, &mut t);
        let (rate, hz) = (44_100.0f32, 440.0f32);
        let mut ph = [0.0f32; 2];
        let x: std::vec::Vec<f32> = (0..32768).map(|_| sample(&t, &mut ph, hz / rate, 0.0, 1.0).0).collect();
        let power = |f: f64| {
            let (mut re, mut im) = (0.0f64, 0.0f64);
            for (i, v) in x.iter().enumerate() {
                let a = 2.0 * PI * f * i as f64 / rate as f64;
                re += *v as f64 * a.cos();
                im += *v as f64 * a.sin();
            }
            re * re + im * im
        };
        let low = 440.0 * F0 * BINS / (RATE * 260.0); // 219.6 Hz
        assert!((low - 219.56).abs() < 0.05, "{low}");
        // the partials sit on the harmonics of 219.6 Hz (even ones are louder: 2/(1 + h)), nothing between or below
        for between in [low * 0.5, low * 1.5, low * 2.5] {
            assert!(power(low) > 100.0 * power(between), "{low} vs {between}: {} {}", power(low), power(between));
        }
        assert!(power(low * 2.0) > 100.0 * power(low * 1.5));
    }
}

/// Opt-in parity with the original table (`tools/tf3-parity/build-ref.sh` builds `$TF3_REF/build/pad-ref`; absent →
/// the test passes without checking): the magnitude spectrum, which the random phases do not change, in bands of 16
/// bins, as fractions of the total, for every profile.
#[cfg(all(test, feature = "std"))]
mod parity {
    use super::*;

    fn reference(s: &Shape) -> Option<std::vec::Vec<f32>> {
        let dir = std::env::var("TF3_REF").unwrap_or_else(|_| "/tmp/tf3-ref".into());
        let exe = std::path::Path::new(&dir).join("build/pad-ref");
        if !exe.exists() {
            return None;
        }
        let args = [s.bandwidth, s.scale, s.damp, s.harmonics, s.profile as f32 / 3.0].map(|v| std::format!("{v}"));
        let out = std::process::Command::new(exe).args(args).output().ok()?;
        Some(std::string::String::from_utf8(out.stdout).ok()?.lines().map(|l| l.parse().unwrap()).collect())
    }

    /// Power per band of 16 bins over the first 4096 bins (where the partials are), as fractions of their sum.
    fn bands(t: &[f32]) -> std::vec::Vec<f64> {
        let mut c = std::vec![0.0f32; 2 * LEN];
        for (i, v) in t.iter().enumerate() {
            c[2 * i] = *v;
        }
        ifft(&mut c);
        let p: std::vec::Vec<f64> = (0..4096).map(|k| (c[2 * k] as f64).powi(2) + (c[2 * k + 1] as f64).powi(2)).collect();
        let b: std::vec::Vec<f64> = p.chunks(16).map(|x| x.iter().sum()).collect();
        let total: f64 = b.iter().sum();
        b.into_iter().map(|x| x / total).collect()
    }

    #[test]
    fn the_magnitude_spectrum_matches_the_original() {
        let cases = [
            Shape { bandwidth: 0.5, scale: 0.1, damp: 0.5, harmonics: 0.1, profile: GAUSS },
            Shape { bandwidth: 0.3, scale: 0.3, damp: 0.2, harmonics: 0.5, profile: GAUSS },
            Shape { bandwidth: 0.6, scale: 0.0, damp: 0.8, harmonics: 0.2, profile: SINGLE },
            Shape { bandwidth: 0.5, scale: 0.1, damp: 0.5, harmonics: 0.1, profile: DETUNE },
            Shape { bandwidth: 0.2, scale: 0.2, damp: 0.0, harmonics: 1.0, profile: SPREAD },
            // the synthetic parity items' patches (tools/unisynth-parity/tf3.ts)
            Shape { bandwidth: 0.3, scale: 0.1, damp: 0.5, harmonics: 0.2, profile: SINGLE },
            Shape { bandwidth: 0.3, scale: 0.1, damp: 0.5, harmonics: 0.2, profile: GAUSS },
        ];
        for s in cases {
            let Some(want) = reference(&s) else { return };
            let mut t = std::vec![0.0f32; LEN];
            build(&s, 11, &mut t);
            let (a, b) = (bands(&t), bands(&want));
            // the largest band difference relative to the largest band
            let top = b.iter().cloned().fold(0.0f64, f64::max);
            let err = a.iter().zip(&b).map(|(x, y)| (x - y).abs()).fold(0.0f64, f64::max) / top;
            std::eprintln!("{:?} {}: worst band {:.2} % of the peak band", s.profile, s.bandwidth, err * 100.0);
            assert!(err < 0.02, "{s:?}: worst band off by {:.1} % of the peak band", err * 100.0);
        }
    }
}

