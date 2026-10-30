//! Synthetic tests only for `id34` (Hilbert frequency shifter). Expectations come from an independent f64 model
//! written from the behaviour notes (two all-pass chains, sine/cosine mix, signed power-law shift), never from
//! third-party data.
use crate::id34::{Error, Id34, PARAMS, shift_cycles_per_sample, table_sin};

const FS: f64 = 44_100.0;

fn noise(n: usize, seed: u32) -> Vec<f32> {
    let mut s = seed;
    (0..n)
        .map(|_| {
            s = s.wrapping_mul(1664525).wrapping_add(1013904223);
            ((s >> 8) as f32 / (1u32 << 24) as f32 - 0.5) * 1.6
        })
        .collect()
}

fn sine(n: usize, hz: f64, amp: f64) -> Vec<f32> {
    (0..n).map(|i| (amp * (2.0 * core::f64::consts::PI * hz * i as f64 / FS).sin()) as f32).collect()
}

fn run(p: &[f32; PARAMS], l: &[f32], r: &[f32]) -> (Vec<f32>, Vec<f32>) {
    let mut d = Id34::new(p).expect("supported");
    let (mut a, mut b) = (l.to_vec(), r.to_vec());
    d.process(&mut a, &mut b);
    (a, b)
}

/// Signed upward shift in cycles/sample from P3, f64: `±(2p-1)^8`, up for p < 0.5.
fn shift64(p3: f64) -> f64 {
    let t = (2.0 * p3 - 1.0).powi(8);
    if p3 < 0.5 { t } else { -t }
}

/// Independent f64 model, one channel: all-pass chain A (undelayed) and chain B (input delayed one sample), each
/// section `y = c·x − x₂ + f·y₂` with `c = a²` of the published polyphase coefficients; output `cos θ·A + sin θ·B`,
/// `θ = −2π·n·shift` for sample n ≥ 1. Section 1 of chain A has feedback 0.1617158 (observed), not a².
fn model(p3: f64, x: &[f32]) -> Vec<f64> {
    // (c, feedback, [x1, x2, y1, y2]) per section
    let chain = |a: [f64; 4], first_fb: f64| -> Vec<(f64, f64, [f64; 4])> {
        a.iter().enumerate().map(|(k, &a)| (a * a, if k == 0 { first_fb } else { a * a }, [0.0; 4])).collect()
    };
    let mut ca = chain([0.4021921162426, 0.8561710882420, 0.9722909545651, 0.9952884791278], 0.161_715_8);
    let mut cb = chain([0.6923878, 0.9360654322959, 0.9882295226860, 0.9987488452737], 0.6923878 * 0.6923878);
    fn tick(c: &mut [(f64, f64, [f64; 4])], mut v: f64) -> f64 {
        for (c0, fb, s) in c.iter_mut() {
            let y = *c0 * v - s[1] + *fb * s[3];
            *s = [v, s[0], y, s[2]];
            v = y;
        }
        v
    }
    let s = shift64(p3);
    let mut prev = 0.0;
    x.iter()
        .enumerate()
        .map(|(n, &xi)| {
            let xi = xi as f64;
            let (a, b) = (tick(&mut ca, xi), tick(&mut cb, prev));
            prev = xi;
            let th = -2.0 * core::f64::consts::PI * (n + 1) as f64 * s;
            th.cos() * a + th.sin() * b
        })
        .collect()
}

fn max_err(got: &[f32], want: &[f64]) -> f64 {
    got.iter().zip(want).map(|(&g, &w)| (g as f64 - w).abs()).fold(0.0, f64::max)
}

/// Hann-windowed single-frequency magnitude (amplitude units).
fn mag(sig: &[f32], hz: f64) -> f64 {
    let n = sig.len() as f64;
    let (mut re, mut im) = (0.0, 0.0);
    for (i, &v) in sig.iter().enumerate() {
        let w = 0.5 - 0.5 * (2.0 * core::f64::consts::PI * i as f64 / n).cos();
        let ph = 2.0 * core::f64::consts::PI * hz * i as f64 / FS;
        re += v as f64 * w * ph.cos();
        im += v as f64 * w * ph.sin();
    }
    2.0 * re.hypot(im) / (n * 0.5)
}

#[test]
fn rejects_bad_values_explicitly() {
    for i in 0..PARAMS {
        for v in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, -0.001, 1.001] {
            let mut d = Id34::new(&[0.1, 0.2, 0.3, 0.4]).unwrap();
            assert_eq!(d.set_param(i, v), Err(Error::Range), "set_param({i}, {v})");
            let mut p = [0.1, 0.2, 0.3, 0.4];
            p[i] = v;
            assert_eq!(Id34::new(&p).err(), Some(Error::Range));
        }
    }
    let mut d = Id34::new(&[0.0; 4]).unwrap();
    assert_eq!(d.set_param(4, 0.5), Err(Error::Index));
    assert_eq!(d.set_param(usize::MAX, 0.5), Err(Error::Index));
}

#[test]
fn accepts_the_full_unit_range() {
    for v in [0.0, 1.0, 0.5, 0.001, 0.999, 128.0 / 255.0] {
        for i in 0..PARAMS {
            let mut d = Id34::new(&[0.5; 4]).unwrap();
            assert_eq!(d.set_param(i, v), Ok(()));
        }
    }
}

#[test]
fn rejection_leaves_the_device_untouched() {
    let (l, r) = (noise(300, 1), noise(300, 2));
    let p = [0.1, 0.2, 0.3, 0.2];
    let (want_l, want_r) = run(&p, &l, &r);
    let mut d = Id34::new(&p).unwrap();
    for (i, v) in [(3, f32::NAN), (3, 1.5), (3, -1.0), (4, 0.5), (9, 0.5)] {
        assert!(d.set_param(i, v).is_err());
    }
    let (mut a, mut b) = (l.clone(), r.clone());
    d.process(&mut a, &mut b);
    assert_eq!((a, b), (want_l, want_r));
}

#[test]
fn only_parameter_three_matters() {
    let (l, r) = (noise(512, 3), noise(512, 4));
    let want = run(&[0.0, 0.0, 0.0, 0.2], &l, &r);
    for p in [[1.0, 0.3, 0.7, 0.2], [0.5, 1.0, 0.0, 0.2], [0.123, 0.456, 0.789, 0.2]] {
        assert_eq!(run(&p, &l, &r), want, "{p:?}");
    }
    assert_ne!(run(&[0.0, 0.0, 0.0, 0.25], &l, &r), want);
}

#[test]
fn shift_law_is_a_signed_eighth_power() {
    assert_eq!(shift_cycles_per_sample(0.5), 0.0);
    assert_eq!(shift_cycles_per_sample(0.0), 1.0);
    assert_eq!(shift_cycles_per_sample(1.0), -1.0);
    for k in 0..=100 {
        let p = k as f32 / 100.0;
        let want = shift64(p as f64);
        let got = shift_cycles_per_sample(p) as f64;
        assert!((got - want).abs() <= 1e-6 * want.abs() + 1e-12, "p={p}: {got} vs {want}");
    }
    // steep: 1 % off centre is nowhere near 1 % of full scale
    assert!(shift_cycles_per_sample(0.49).abs() < 1e-12);
    // upward below 0.5, downward above
    assert!(shift_cycles_per_sample(0.3) > 0.0 && shift_cycles_per_sample(0.7) < 0.0);
    // the song's value 0.8549 is about 2.8 kHz at 44.1 kHz
    let hz = shift_cycles_per_sample(0.854_901_97) as f64 * FS;
    assert!((hz + 2840.0).abs() < 5.0, "{hz}");
}

#[test]
fn table_sine_follows_the_sine_within_interpolation_error() {
    let mut worst = 0.0f64;
    for k in -4000..=4000 {
        let x = k as f32 * 0.0123;
        let e = (table_sin(x) as f64 - (x as f64).sin()).abs();
        worst = worst.max(e);
    }
    // linear interpolation of a 512-entry table: (2π/512)²/8 = 1.9e-5, plus f32 argument rounding
    assert!(worst < 3e-5, "{worst}");
    assert_eq!(table_sin(0.0), 0.0);
    assert!((table_sin(core::f32::consts::FRAC_PI_2) - 1.0).abs() < 2e-5);
}

#[test]
fn matches_independent_model() {
    let n = 256;
    let x = noise(n, 7);
    for p3 in [0.5f32, 0.2, 0.0, 0.35, 0.9, 0.7] {
        let (l, r) = run(&[0.0, 0.0, 0.0, p3], &x, &noise(n, 8));
        assert!(max_err(&l, &model(p3 as f64, &x)) < 6e-4, "p3={p3} L err {}", max_err(&l, &model(p3 as f64, &x)));
        let xr = noise(n, 8);
        assert!(max_err(&r, &model(p3 as f64, &xr)) < 6e-4, "p3={p3} R");
    }
}

#[test]
fn channels_are_independent() {
    let (l, r) = (noise(300, 11), noise(300, 12));
    let (a, b) = run(&[0.0, 0.0, 0.0, 0.3], &l, &r);
    let z = vec![0.0; 300];
    assert_eq!(run(&[0.0, 0.0, 0.0, 0.3], &l, &z).0, a);
    assert_eq!(run(&[0.0, 0.0, 0.0, 0.3], &z, &r).1, b);
    assert!(run(&[0.0, 0.0, 0.0, 0.3], &l, &z).1.iter().all(|&v| v == 0.0));
}

/// Single-sideband property: a sine comes out at f0 + shift (one sideband), the mirror image is ≥ 30 dB down.
#[test]
fn shifts_a_sine_up_below_half_and_down_above_half() {
    let f0 = 3000.0;
    for (p3, up) in [(0.2f32, true), (0.1, true), (0.8, false), (0.9, false)] {
        let hz = shift_cycles_per_sample(p3) as f64 * FS; // signed, positive = up
        let x = sine(8192, f0, 0.5);
        let (y, _) = run(&[0.0, 0.0, 0.0, p3], &x, &x);
        let y = &y[2048..6144];
        let (want, image) = (mag(y, f0 + hz), mag(y, f0 - hz));
        assert_eq!(hz > 0.0, up);
        assert!(want > 0.45 && want < 0.55, "p3={p3}: wanted sideband {want}");
        assert!(image < want / 30.0, "p3={p3}: image {image} vs {want}");
    }
}

#[test]
fn zero_shift_is_a_unity_gain_phase_rotation() {
    let x = sine(8192, 1000.0, 0.5);
    let (y, _) = run(&[0.0, 0.0, 0.0, 0.5], &x, &x);
    let m = mag(&y[2048..6144], 1000.0);
    assert!((m - 0.5).abs() < 0.01, "{m}");
}

#[test]
fn block_splitting_is_bit_identical_and_state_survives_set_param() {
    let (l, r) = (noise(512, 21), noise(512, 22));
    let p = [0.0, 0.0, 0.0, 0.15];
    let (want_l, want_r) = run(&p, &l, &r);
    let mut d = Id34::new(&p).unwrap();
    let (mut a, mut b) = (l.clone(), r.clone());
    for k in 0..4 {
        let s = k * 128..(k + 1) * 128;
        d.set_param(3, 0.15).unwrap(); // same value: must not reset the phase or the all-pass state
        let (x, y) = (&mut a[s.clone()], &mut b[s]);
        d.process(x, y);
    }
    assert_eq!((a, b), (want_l, want_r));
}

#[test]
fn shift_changes_apply_from_the_next_block_and_phase_is_continuous() {
    let (l, r) = (noise(256, 31), noise(256, 32));
    let mut d = Id34::new(&[0.0, 0.0, 0.0, 0.2]).unwrap();
    let (mut a, mut b) = (l.clone(), r.clone());
    d.process(&mut a[..128], &mut b[..128]);
    let ref1 = run(&[0.0, 0.0, 0.0, 0.2], &l, &r);
    assert_eq!(&a[..128], &ref1.0[..128], "first block uses the first setting");
    d.set_param(3, 0.9).unwrap();
    d.process(&mut a[128..], &mut b[128..]);
    assert_ne!(&a[128..], &ref1.0[128..], "second block uses the new shift");
    assert!(a.iter().chain(b.iter()).all(|v| v.is_finite()));
}

#[test]
fn silence_stays_silent() {
    let z = vec![0.0; 1000];
    for p3 in [0.0, 0.3, 0.5, 0.8, 1.0] {
        let (l, r) = run(&[0.0, 0.0, 0.0, p3], &z, &z);
        assert!(l.iter().chain(r.iter()).all(|&v| v == 0.0), "p3={p3}");
    }
}

#[test]
#[should_panic(expected = "channel length mismatch")]
fn length_mismatch_panics() {
    let mut d = Id34::new(&[0.0; 4]).unwrap();
    d.process(&mut [0.0; 4], &mut [0.0; 3]);
}
