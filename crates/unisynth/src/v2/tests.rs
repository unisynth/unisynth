use super::*;

#[test]
fn fast_functions_follow_the_exact_ones() {
    for i in -100..=100 {
        let x = i as f32 * 0.0157; // −π/2..π/2
        assert!((fastsin(x) - libm::sinf(x)).abs() < 2e-4, "{x}");
    }
    for i in -40..300 {
        let x = i as f32 * 0.05; // negative arguments below −π/2 are V2's bug, kept
        assert!((fastsinrc(x) - libm::sinf(x)).abs() < 3e-4, "{x}");
    }
    for i in -199..200 {
        let x = i as f32 * 0.01; // the table V2 picks is right only below 2
        assert!((fastatan(x) - libm::atanf(x)).abs() < 2e-3, "{x}: {} vs {}", fastatan(x), libm::atanf(x));
    }
    assert_eq!(calcfreq(1.0), 1.0);
    assert!((calcfreq(0.5) - 1.0 / 32.0).abs() < 1e-6);
    assert!((calcfreq2(0.0) - 1.0 / 2048.0).abs() < 1e-8);
}

#[test]
fn fixed_point_helpers() {
    assert_eq!(utof23(0), 0.0);
    assert_eq!(utof23(0x8000_0000), 0.5);
    assert!((utof23(u32::MAX) - 1.0).abs() < 1.2e-7);
    assert_eq!(ftou32(0.5), 0x8000_0000);
    assert_eq!(ftou32(0.25), 0x4000_0000);
    let mut s = 1;
    let v: Vec<f32> = (0..1000).map(|_| frandom(&mut s)).collect();
    assert!(v.iter().all(|x| (-1.0..1.0).contains(x)));
    assert!(v.iter().sum::<f32>().abs() < 60.0);
}

/// A 7-bit-law filter through the shared formulas (`flt::run`), as the drive's filter modes run it (`mode` 5..=11).
#[cfg(any(feature = "us-flt-feedback", feature = "us-drive", feature = "us-strip"))]
struct Flt {
    k: crate::flt::Kind,
    c: [f32; crate::flt::NC],
    s: [f32; crate::flt::NS],
}

#[cfg(any(feature = "us-flt-feedback", feature = "us-drive", feature = "us-strip"))]
impl Flt {
    fn new(mode: u8, cut: f32, res: f32) -> Flt {
        let k = crate::flt::STEPS_KINDS[(mode - dmode::FLT_BASE - 1) as usize];
        let c = crate::flt::coefs_steps(k, cut / 128.0, res / 128.0, 44_100.0, crate::flt::Forms::STEPS);
        Flt { k, c, s: [0.0; crate::flt::NS] }
    }
    fn run(&mut self, x: f32) -> f32 {
        crate::flt::run(self.k, &self.c, &mut self.s, x)
    }
}

/// The drive's filter modes (`dmode::FLT_BASE` + 1..=7).
#[cfg(any(feature = "us-flt-feedback", feature = "us-drive", feature = "us-strip"))]
const LOW: u8 = 5;
#[cfg(any(feature = "us-flt-feedback", feature = "us-drive", feature = "us-strip"))]
const ALL: u8 = 9;
#[cfg(any(feature = "us-flt-feedback", feature = "us-drive", feature = "us-strip"))]
const MOOG_L: u8 = 10;
#[cfg(any(feature = "us-flt-feedback", feature = "us-drive", feature = "us-strip"))]
const MOOG_H: u8 = 11;

#[test]
#[cfg(any(feature = "us-flt-feedback", feature = "us-drive", feature = "us-strip"))]
fn lrc_low_pass_passes_dc_and_cuts_highs() {
    let mut f = Flt::new(LOW, 64.0, 0.0);
    let mut y = 0.0;
    for _ in 0..20_000 {
        y = f.run(0.5);
    }
    assert!((y - 0.5).abs() < 1e-3, "{y}");
    // Nyquist-rate alternation is cut hard
    let mut f = Flt::new(LOW, 64.0, 0.0);
    let mut m: f32 = 0.0;
    for i in 0..4000 {
        let y = f.run(if i % 2 == 0 { 1.0 } else { -1.0 });
        if i > 3000 {
            m = m.max(y.abs());
        }
    }
    assert!(m < 0.01, "{m}");
    // the all-pass keeps the level of noise
    let mut s = 7;
    let (mut e_in, mut e_out) = (0.0f64, 0.0f64);
    let mut f = Flt::new(ALL, 40.0, 30.0);
    for i in 0..200_000 {
        let x = frandom(&mut s) * 0.5;
        let y = f.run(x);
        if i > 1000 {
            e_in += (x * x) as f64;
            e_out += (y * y) as f64;
        }
    }
    assert!((10.0 * (e_out / e_in).log10()).abs() < 0.5, "{}", 10.0 * (e_out / e_in).log10());
}

#[test]
#[cfg(any(feature = "us-flt-feedback", feature = "us-drive", feature = "us-strip"))]
fn moog_low_and_high_split_the_signal() {
    let (mut lo, mut hi) = (Flt::new(MOOG_L, 60.0, 0.0), Flt::new(MOOG_H, 60.0, 0.0));
    for i in 0..5000 {
        let x = if (i / 50) % 2 == 0 { 0.3 } else { -0.3 };
        let (a, b) = (lo.run(x), hi.run(x));
        assert!((a + b - x).abs() < 1e-5, "high-pass = input − low-pass");
    }
}

/// The Moog low-pass passes a tone well under its cutoff and cuts one well over it (V2's 4-pole, 2× oversampled);
/// the sum check above holds for any filter, so this one pins the response.
#[test]
#[cfg(any(feature = "us-flt-feedback", feature = "us-drive", feature = "us-strip"))]
fn moog_low_pass_passes_lows_and_cuts_highs() {
    let rms = |hz: f32| {
        let mut f = Flt::new(MOOG_L, 64.0, 0.0);
        let y: Vec<f32> = (0..8820).map(|i| f.run(0.25 * (core::f32::consts::TAU * hz * i as f32 / 44_100.0).sin())).collect();
        (y[4410..].iter().map(|v| v * v).sum::<f32>() / 4410.0).sqrt() / (0.25 * core::f32::consts::FRAC_1_SQRT_2)
    };
    let (low, high) = (rms(60.0), rms(12_000.0));
    assert!(low > 0.7 && low < 1.3, "60 Hz passes: {low}");
    assert!(high < 0.05, "12 kHz is cut: {high}");
}

#[test]
fn distortion_modes() {
    let od = DistP::new(1.0, 40.0, 128.0, 64.0, 44_100.0); // gain 1.41: V2's atan is right below 2
    let mut d = Dist::default();
    let (l, r) = d.run(&od, 1.0, -1.0);
    assert!(l > 0.9 && l <= 1.01 && (l + r).abs() < 1e-6, "{l} {r}");
    let clip = DistP::new(2.0, 64.0, 128.0, 64.0, 44_100.0);
    assert_eq!(d.run(&clip, 0.9, -0.2), (1.0, -0.8));
    // crusher: param 1 = 0 is a 16-bit quantiser at unit input gain
    let cr = DistP::new(3.0, 32.0, 0.0, 0.0, 44_100.0);
    let (q, _) = d.run(&cr, 0.123_456, 0.0);
    assert!((q - 0.123_456).abs() < 1.0 / 32_768.0);
    // decimator holds the value until its counter wraps
    let dec = DistP::new(4.0, 32.0, 64.0, 0.0, 44_100.0);
    let mut d = Dist::default();
    let held: Vec<f32> = (0..100).map(|i| d.run(&dec, i as f32, 0.0).0).collect();
    assert!(held.windows(2).filter(|w| w[0] != w[1]).count() < 10, "{held:?}");
    assert_eq!(DistP::new(0.0, 32.0, 0.0, 0.0, 44_100.0).mode, dmode::OFF);
}
