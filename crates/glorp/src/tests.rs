//! Synthetic tests only: expectations come from an independent f64 model written from the behaviour notes
//! (RBJ LP/HP, closed-form distortions), never from third-party data.
use crate::id38::{Error, Id38, PARAMS, distort};

/// LP, cutoff p1, Q = 0.5, one stage, no distortion, envelope neutral, mix 1, gain 1.
fn base() -> [f32; PARAMS] {
    [0.0, 0.5, 0.0, 0.5, 0.0, 0.0, 0.0, 0.5, 0.01, 0.1, 0.8, 0.2, 0.5, 1.0, 0.5]
}

fn with(edits: &[(usize, f32)]) -> [f32; PARAMS] {
    let mut p = base();
    for &(i, v) in edits {
        p[i] = v;
    }
    p
}

fn noise(n: usize, seed: u32) -> Vec<f32> {
    let mut s = seed;
    (0..n)
        .map(|_| {
            s = s.wrapping_mul(1664525).wrapping_add(1013904223);
            ((s >> 8) as f32 / (1u32 << 24) as f32 - 0.5) * 1.6
        })
        .collect()
}

fn run(p: &[f32; PARAMS], l: &[f32], r: &[f32]) -> (Vec<f32>, Vec<f32>) {
    let mut d = Id38::new(p).expect("supported");
    let (mut a, mut b) = (l.to_vec(), r.to_vec());
    d.process(&mut a, &mut b);
    (a, b)
}

fn r64(u: f64) -> f64 {
    let u2 = u * u;
    u * (378.0 * u2 * u2 + 17325.0 * u2 + 135135.0) / (18.0 * u2 * u2 * u2 + 3150.0 * u2 * u2 + 62370.0 * u2 + 135135.0)
}

fn dist64(kind: u32, x: f64, drive: f64) -> f64 {
    let k = drive * drive * 20.0 + 0.101;
    match kind {
        0 => x,
        1 => r64(k * x) / r64(k),
        2 => {
            let g = |u: f64| u * (u * u + 27.0) / (9.0 * u * u + 27.0);
            g(k * x) / g(k)
        }
        _ => unreachable!(),
    }
}

/// Independent f64 model of the supported subset, one channel.
fn model(p: &[f32; PARAMS], x: &[f32]) -> Vec<f64> {
    let p: Vec<f64> = p.iter().map(|&v| v as f64).collect();
    let mode = (p[0] * 11.999) as u32;
    let post = (p[6] * 5.999) as u32;
    let q = 0.5 + 29.5 * p[2] * p[2];
    let fc = (p[1] * p[1] * 19980.0 + 20.0).clamp(20.0, 20000.0);
    let w = 2.0 * core::f64::consts::PI * fc / 44100.0;
    let alpha = w.sin() / (2.0 * q);
    let (b0, b1) = if mode == 0 { ((1.0 - w.cos()) / 2.0, 1.0 - w.cos()) } else { ((1.0 + w.cos()) / 2.0, -1.0 - w.cos()) };
    let (a0, a1, a2) = (1.0 + alpha, -2.0 * w.cos(), 1.0 - alpha);
    let g = if p[14] <= 0.5 { 2.0 * p[14] } else { 4.0 * p[14] - 1.5 };
    let (mut x1, mut x2, mut y1, mut y2) = (0.0, 0.0, 0.0, 0.0);
    x.iter()
        .map(|&xi| {
            let xi = xi as f64;
            let y = (b0 * xi + b1 * x1 + b0 * x2 - a1 * y1 - a2 * y2) / a0;
            x2 = x1;
            x1 = xi;
            y2 = y1;
            y1 = y;
            let wet = if post > 0 { dist64(post, y, p[7]) } else { y };
            ((wet - xi) * p[13] + xi) * g
        })
        .collect()
}

fn max_err(got: &[f32], want: &[f64]) -> f64 {
    got.iter().zip(want).map(|(&g, &w)| (g as f64 - w).abs()).fold(0.0, f64::max)
}

#[test]
fn rejects_unsupported_settings_explicitly() {
    let cases: &[(usize, f32, Error)] = &[
        (0, 0.2, Error::Mode),   // 2.39 -> 2 (ladder family)
        (0, 1.0, Error::Mode),   // 11
        (4, 0.2, Error::Stages), // 2 stages
        (4, 1.0, Error::Stages),
        (5, 0.2, Error::PreDistortion), // pre type 1
        (5, 1.0, Error::PreDistortion),
        (6, 0.6, Error::PostDistortion), // type 3
        (6, 1.0, Error::PostDistortion),
        (12, 0.6, Error::Envelope),
        (12, 0.4, Error::Envelope),
        (12, 128.0 / 255.0, Error::Envelope), // automation byte nearest 0.5 is still not neutral
        (1, f32::NAN, Error::Range),
        (1, -0.1, Error::Range),
        (13, 1.1, Error::Range),
        (14, f32::INFINITY, Error::Range),
        (15, 0.5, Error::Index),
    ];
    for &(i, v, e) in cases {
        let mut d = Id38::new(&base()).unwrap();
        assert_eq!(d.set_param(i, v), Err(e), "set_param({i}, {v})");
        let mut p = base();
        if i < PARAMS {
            p[i] = v;
            assert_eq!(Id38::new(&p).err(), Some(e), "new with p[{i}]={v}");
        }
    }
}

#[test]
fn accepts_the_song_subset() {
    // modes 0/1, post 0/1/2, drive and the unused slots (3, 8..=11) free, stage 1 up to the last value below 2.
    for p0 in [0.0, 0.05, 0.0834, 0.0835, 0.16] {
        assert!(Id38::new(&with(&[(0, p0)])).is_ok(), "p0={p0}");
    }
    for p6 in [0.0, 0.16, 0.17, 0.33, 0.5] {
        assert!(Id38::new(&with(&[(6, p6)])).is_ok(), "p6={p6}");
    }
    assert!(Id38::new(&with(&[(3, 0.9), (4, 0.12), (7, 1.0), (8, 0.0), (9, 1.0), (10, 0.3), (11, 0.7)])).is_ok());
    for i in 0..PARAMS {
        let mut d = Id38::new(&base()).unwrap();
        assert!(d.set_param(i, base()[i]).is_ok());
    }
}

#[test]
fn rejection_leaves_the_device_untouched() {
    let (l, r) = (noise(300, 1), noise(300, 2));
    let (want_l, want_r) = run(&with(&[(6, 0.5)]), &l, &r);
    let mut d = Id38::new(&with(&[(6, 0.5)])).unwrap();
    for (i, v) in [(0, 0.5), (4, 0.5), (5, 0.5), (6, 0.9), (12, 0.9), (1, f32::NAN), (99, 0.0)] {
        assert!(d.set_param(i, v).is_err());
    }
    let (mut a, mut b) = (l.clone(), r.clone());
    d.process(&mut a, &mut b);
    assert_eq!((a, b), (want_l, want_r));
}

#[test]
fn filters_match_the_f64_model() {
    let x = noise(2000, 7);
    let y = noise(2000, 8);
    let cases = [
        with(&[]),
        with(&[(0, 0.1), (1, 0.3)]), // HP
        with(&[(1, 0.7), (2, 0.5)]),
        with(&[(0, 0.1), (1, 0.8), (2, 0.4), (13, 0.5), (14, 0.9)]),
        with(&[(1, 0.2), (13, 0.0)]),
        with(&[(1, 0.9), (14, 0.2)]),
    ];
    for p in &cases {
        let (a, b) = run(p, &x, &y);
        let peak = model(p, &x).iter().fold(1e-3f64, |m, v| m.max(v.abs()));
        assert!(max_err(&a, &model(p, &x)) < 2e-5 * peak.max(1.0), "left {p:?}");
        assert!(max_err(&b, &model(p, &y)) < 2e-5 * peak.max(1.0), "right {p:?}");
    }
}

#[test]
fn post_distortion_matches_the_f64_model_and_stays_out_of_the_feedback() {
    let x = noise(2000, 3);
    for post_param in [0.2f32, 0.5] {
        // 0.2 -> type 1, 0.5 -> type 2
        for drive in [0.0, 0.3, 0.6, 1.0] {
            let p = with(&[(6, post_param), (7, drive), (1, 0.6), (2, 0.1)]);
            let (a, _) = run(&p, &x, &x);
            let e = max_err(&a, &model(&p, &x));
            // f32 coefficients/state vs f64 model: ~1e-5; a wrong composition (feedback, order) is O(0.1)
            assert!(e < 3e-5, "post {post_param} drive {drive}: err {e:e}");
        }
    }
}

#[test]
fn distortions_are_normalised_odd_and_closed_form() {
    for drive in [0.0f32, 0.25, 0.7, 1.0] {
        for kind in [1u32, 2] {
            assert!((distort(kind, 1.0, drive) - 1.0).abs() < 2e-6, "x=1 maps to 1");
            for x in [-1.5f32, -0.4, -0.01, 0.0, 0.2, 0.9, 2.0] {
                let got = distort(kind, x, drive) as f64;
                let want = dist64(kind, x as f64, drive as f64);
                assert!((got - want).abs() < 2e-6 * want.abs().max(1.0), "kind {kind} drive {drive} x {x}: {got} vs {want}");
                assert_eq!(distort(kind, -x, drive), -distort(kind, x, drive), "odd");
            }
        }
        assert_eq!(distort(0, 0.37, drive), 0.37);
    }
}

#[test]
fn dc_gain_gain_law_and_mix() {
    let ones = vec![1.0f32; 4000];
    // LP passes DC, HP blocks it
    let (lp, _) = run(&with(&[]), &ones, &ones);
    assert!((lp[3999] - 1.0).abs() < 1e-4);
    let (hp, _) = run(&with(&[(0, 0.1)]), &ones, &ones);
    assert!(hp[3999].abs() < 1e-4);
    // output gain law: 2p <= .5, else 4p - 1.5
    for (p14, g) in [(0.0, 0.0), (0.25, 0.5), (0.5, 1.0), (0.75, 1.5), (1.0, 2.5)] {
        let (o, _) = run(&with(&[(14, p14)]), &ones, &ones);
        assert!((o[3999] - g).abs() < 2e-4, "p14={p14}: {}", o[3999]);
    }
    // mix 0 = dry * gain exactly
    let x = noise(500, 5);
    let (o, _) = run(&with(&[(13, 0.0), (14, 0.25), (6, 0.5)]), &x, &x);
    for (o, x) in o.iter().zip(&x) {
        assert_eq!(*o, x * 0.5);
    }
}

#[test]
fn block_split_is_bit_identical_and_param_resets_do_not_clear_state() {
    let (l, r) = (noise(1000, 11), noise(1000, 12));
    let p = with(&[(6, 0.5), (1, 0.4), (2, 0.6)]);
    let (wl, wr) = run(&p, &l, &r);
    for chunk in [1usize, 7, 128, 999] {
        let mut d = Id38::new(&p).unwrap();
        let (mut a, mut b) = (l.clone(), r.clone());
        for (ca, cb) in a.chunks_mut(chunk).zip(b.chunks_mut(chunk)) {
            d.set_param(1, 0.4).unwrap(); // same value between blocks: no state loss
            d.process(ca, cb);
        }
        assert_eq!((a, b), (wl.clone(), wr.clone()), "chunk {chunk}");
    }
}

#[test]
fn param_change_between_blocks_keeps_filter_state() {
    let x = noise(256, 21);
    let mut d = Id38::new(&base()).unwrap();
    let (mut a, mut b) = (x[..128].to_vec(), x[..128].to_vec());
    d.process(&mut a, &mut b);
    d.set_param(1, 0.3).unwrap();
    let (mut a2, mut b2) = (x[128..].to_vec(), x[128..].to_vec());
    d.process(&mut a2, &mut b2);
    // a fresh device started at block 2 differs: the first block's state carried over
    let (fresh, _) = run(&with(&[(1, 0.3)]), &x[128..], &x[128..]);
    assert_ne!(a2, fresh);
}

#[test]
fn channels_are_independent_and_lengths_must_match() {
    let x = noise(300, 31);
    let zeros = vec![0.0f32; 300];
    let (a, b) = run(&with(&[(6, 0.5)]), &x, &zeros);
    let (a1, _) = run(&with(&[(6, 0.5)]), &x, &x);
    assert_eq!(a, a1);
    assert!(b.iter().all(|&v| v == 0.0));
    let mut d = Id38::new(&base()).unwrap();
    d.process(&mut [], &mut []);
}

#[test]
#[should_panic]
fn mismatched_lengths_panic() {
    let mut d = Id38::new(&base()).unwrap();
    d.process(&mut [0.0; 4], &mut [0.0; 3]);
}
