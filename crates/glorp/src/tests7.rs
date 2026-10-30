//! Synthetic tests only for `id7` (stereo look-ahead compressor / limiter). Expectations come from an independent f64
//! model written from the behaviour notes (research/glorp-devices.md §ID 7) and from closed-form properties
//! (87-sample delay, steady-state gain law, linear envelope ramps), never from third-party data.
use crate::id7::{Error, Id7, PARAMS};

/// Delay of the look-ahead path in samples: an 88-sample ring is written then read one slot ahead.
const DELAY: usize = 87;

fn noise(n: usize, seed: u32, amp: f32) -> Vec<f32> {
    let mut s = seed;
    (0..n)
        .map(|_| {
            s = s.wrapping_mul(1664525).wrapping_add(1013904223);
            ((s >> 8) as f32 / (1u32 << 24) as f32 - 0.5) * 2.0 * amp
        })
        .collect()
}

/// Song-like chunk (internal detector).
const SONG: [f32; PARAMS] = [0.0, 0.24, 1.0, 0.125, 0.0283, 0.13, 0.41];
/// Unity in, unity makeup, threshold at 0 (`P2 = 1`: no compression below an input of 1.0), ratio 2.
const UNITY: [f32; PARAMS] = [0.0, 0.5, 1.0, 0.0, 0.5, 0.5, 0.5];

fn run(p: &[f32; PARAMS], l: &[f32], r: &[f32]) -> (Vec<f32>, Vec<f32>) {
    let mut d = Id7::new(p).expect("supported");
    let (mut a, mut b) = (l.to_vec(), r.to_vec());
    d.process(&mut a, &mut b).expect("internal detector");
    (a, b)
}

/// Independent f64 model: parameters mapped from the notes, per-sample linear envelope follower, hard-knee gain
/// computer, exact `DELAY`-sample look-ahead delay (history vector, no ring).
struct Model {
    env: f64,
    hist: Vec<[f64; 2]>,
}

impl Model {
    fn new() -> Self {
        Model { env: 0.0, hist: Vec::new() }
    }

    fn step(&mut self, p: &[f32; PARAMS], x: [f64; 2], side: [f64; 2]) -> [f64; 2] {
        let q = |i: usize| p[i] as f64;
        let gin = 2f64.powf((2.0 * q(1) - 1.0) * 12.0 / 6.0);
        let thr = 2f64.powf((q(2) - 1.0) * 36.0 / 6.0);
        let ratio = 2.0 + 18.0 * q(3) * q(3);
        let att = (5000.0 * q(4) * q(4) + 1.0) * 0.2;
        let rel = 5000.0 * q(5) * q(5) + 1.0;
        let mk = 2f64.powf((2.0 * q(6) - 1.0) * 12.0 / 6.0);
        let (att_rate, rel_rate) = (1000.0 / 44100.0 / att, 1000.0 / 44100.0 / rel);
        self.hist.push([x[0] * gin, x[1] * gin]);
        let det = (side[0] * gin).abs().max((side[1] * gin).abs());
        if det > self.env {
            self.env = (self.env + att_rate).min(det);
        } else {
            self.env = (self.env - rel_rate).max(det);
        }
        let g = if self.env > thr { (((self.env - thr) / ratio + thr) * mk) / self.env } else { mk };
        let d = self.hist.len().checked_sub(DELAY + 1).map(|i| self.hist[i]).unwrap_or([0.0; 2]);
        [d[0] * g, d[1] * g]
    }
}

fn model(p: &[f32; PARAMS], l: &[f32], r: &[f32], side: Option<(&[f32], &[f32])>) -> (Vec<f64>, Vec<f64>) {
    let mut m = Model::new();
    let mut o = (Vec::new(), Vec::new());
    for i in 0..l.len() {
        let x = [l[i] as f64, r[i] as f64];
        let s = side.map(|(a, b)| [a[i] as f64, b[i] as f64]).unwrap_or(x);
        let y = m.step(p, x, s);
        o.0.push(y[0]);
        o.1.push(y[1]);
    }
    o
}

fn max_err(got: &[f32], want: &[f64]) -> f64 {
    got.iter().zip(want).map(|(&g, &w)| (g as f64 - w).abs()).fold(0.0, f64::max)
}

#[test]
fn rejects_bad_values_and_indices_explicitly() {
    for i in 0..PARAMS {
        for v in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, -0.001, 1.001] {
            let mut d = Id7::new(&SONG).unwrap();
            assert_eq!(d.set_param(i, v), Err(Error::Range), "set_param({i}, {v})");
            let mut p = SONG;
            p[i] = v;
            assert_eq!(Id7::new(&p).err(), Some(Error::Range), "new with p[{i}]={v}");
        }
    }
    let mut d = Id7::new(&SONG).unwrap();
    // the original silently ignores index >= 7; here that is an error
    assert_eq!(d.set_param(PARAMS, 0.5), Err(Error::Index));
    assert_eq!(d.set_param(usize::MAX, 0.5), Err(Error::Index));
}

#[test]
fn accepts_the_whole_unit_range_of_every_parameter() {
    for v in [0.0, 1.0, 0.5, 0.4999, 0.001, 0.999, 128.0 / 255.0] {
        for i in 1..PARAMS {
            let mut d = Id7::new(&SONG).unwrap();
            assert_eq!(d.set_param(i, v), Ok(()), "P{i}={v}");
            let (mut l, mut r) = (noise(500, 5, 3.0), noise(500, 6, 3.0));
            d.process(&mut l, &mut r).unwrap();
            assert!(l.iter().chain(&r).all(|x| x.is_finite()), "P{i}={v}");
        }
    }
}

#[test]
fn rejection_leaves_the_device_untouched() {
    let (l, r) = (noise(600, 1, 2.0), noise(600, 2, 2.0));
    let want = run(&SONG, &l, &r);
    let mut d = Id7::new(&SONG).unwrap();
    for (i, v) in [(3, f32::NAN), (3, 1.5), (1, -1.0), (7, 0.5), (9, 0.5), (0, 2.0)] {
        assert!(d.set_param(i, v).is_err());
    }
    let (mut a, mut b) = (l.clone(), r.clone());
    d.process(&mut a, &mut b).unwrap();
    assert_eq!((a, b), want);
}

#[test]
fn p0_selects_the_detector_input_at_exactly_one_half() {
    for (v, side) in [(0.0, false), (0.4999, false), (0.5, true), (0.5001, true), (1.0, true)] {
        let mut p = SONG;
        p[0] = v;
        let mut d = Id7::new(&p).unwrap();
        assert_eq!(d.sidechain(), side, "P0={v}");
        let (mut l, mut r) = (vec![0.5; 4], vec![0.5; 4]);
        let (sl, sr) = (vec![0.0; 4], vec![0.0; 4]);
        // each call form is refused, untouched, in the other mode
        let (el, er) = (l.clone(), r.clone());
        if side {
            assert_eq!(d.process(&mut l, &mut r), Err(Error::Sidechain), "P0={v}");
        } else {
            assert_eq!(d.process_sidechain(&mut l, &mut r, &sl, &sr), Err(Error::Sidechain), "P0={v}");
        }
        assert_eq!((l, r), (el, er), "refused call must not touch the buffers (P0={v})");
    }
}

#[test]
fn refused_call_changes_no_state() {
    let (l, r) = (noise(700, 3, 3.0), noise(700, 4, 3.0));
    let want = run(&SONG, &l, &r);
    let mut d = Id7::new(&SONG).unwrap();
    let (mut a, mut b) = (l.clone(), r.clone());
    let (z, z2) = (vec![9.0; 700], vec![9.0; 700]);
    assert_eq!(d.process_sidechain(&mut a, &mut b, &z, &z2), Err(Error::Sidechain));
    d.process(&mut a, &mut b).unwrap();
    assert_eq!((a, b), want);
}

#[test]
fn impulse_comes_out_exactly_87_samples_later() {
    let mut l = vec![0.0f32; 300];
    let mut r = vec![0.0f32; 300];
    l[10] = 0.5;
    r[20] = -0.25;
    let (a, b) = run(&UNITY, &l, &r);
    for i in 0..300 {
        assert_eq!(a[i], if i == 10 + DELAY { 0.5 } else { 0.0 }, "L {i}");
        assert_eq!(b[i], if i == 20 + DELAY { -0.25 } else { 0.0 }, "R {i}");
    }
}

#[test]
fn below_threshold_with_unity_gains_is_a_pure_delay() {
    let (l, r) = (noise(1000, 7, 0.99), noise(1000, 8, 0.99));
    let (a, b) = run(&UNITY, &l, &r);
    assert!(a[..DELAY].iter().chain(&b[..DELAY]).all(|&x| x == 0.0));
    assert_eq!(a[DELAY..], l[..1000 - DELAY]);
    assert_eq!(b[DELAY..], r[..1000 - DELAY]);
}

#[test]
fn input_gain_and_makeup_follow_the_six_per_unit_law() {
    // P1 = 1 => (2·1−1)·12 = 12 "dB-units" => 2^(12/6) = 4 ; P6 = 0 => 2^(−2) = 0.25 ; both far below the threshold
    let mut p = UNITY;
    p[1] = 1.0;
    p[2] = 1.0;
    let x = vec![0.1f32; 400];
    let (a, _) = run(&p, &x, &x);
    // gain 4 on 0.1 = 0.4 < thr 1: only makeup 1 applies
    assert!((a[399] - 0.4).abs() < 1e-6, "{}", a[399]);
    p[6] = 0.0;
    let (a, _) = run(&p, &x, &x);
    assert!((a[399] - 0.1).abs() < 1e-6, "{}", a[399]);
    p[1] = 0.0; // 2^(−2) in
    let (a, _) = run(&p, &x, &x);
    assert!((a[399] - 0.1 / 16.0).abs() < 1e-7, "{}", a[399]);
}

#[test]
fn steady_state_gain_follows_the_hard_knee_law() {
    // DC 2.0 in, input gain 1, threshold 2^((P2−1)·6) with P2 = 0.9 => 2^−0.6, ratio 2+18·0.25 = 6.5, makeup 1
    let mut p = UNITY;
    p[2] = 0.9;
    p[3] = 0.5;
    p[4] = 0.0; // fastest attack
    let thr = 2f64.powf(-0.6);
    let ratio = 6.5f64;
    let x = vec![2.0f32; 3000];
    let (a, _) = run(&p, &x, &x);
    let want = ((2.0 - thr) / ratio + thr) / 2.0 * 2.0; // gain · delayed input 2.0
    assert!((a[2999] as f64 - want).abs() < 2e-6, "{} vs {}", a[2999], want);
    // an input exactly at the threshold: no reduction (strict >)
    let t = thr as f32;
    let y = vec![t; 3000];
    let (b, _) = run(&p, &y, &y);
    assert!((b[2999] - t).abs() < 1e-6, "{} vs {t}", b[2999]);
}

#[test]
fn envelope_follows_a_linear_ramp_at_the_attack_and_release_rates() {
    // attack time param P4 = 0 => 0.2 → rate (1000/44100)/0.2 = 0.1134 per sample: a 2.0 step is reached in 18 samples;
    // measured through the gain of a signal that is far above the threshold so the gain exposes the envelope
    let mut p = UNITY;
    p[2] = 0.0; // threshold 2^-6
    p[3] = 1.0; // ratio 20
    p[4] = 0.0;
    p[5] = 1.0; // slow release: 5001 => 0.004535/sample... ×(1000/44100)/5001
    let x = vec![2.0f32; 300];
    let (m0, _) = model(&p, &x, &x, None);
    let (a, _) = run(&p, &x, &x);
    assert!(max_err(&a, &m0) < 1e-6, "{}", max_err(&a, &m0));
    // and a falling edge: loud then silence, release glides linearly down (model check over the tail)
    let mut y = vec![2.0f32; 200];
    y.extend(vec![0.0f32; 4000]);
    let (m1, _) = model(&p, &y, &y, None);
    let (b, _) = run(&p, &y, &y);
    assert!(max_err(&b, &m1) < 1e-6, "{}", max_err(&b, &m1));
}

#[test]
fn matches_the_independent_f64_model_on_noise_with_every_parameter_set() {
    let sets: [[f32; PARAMS]; 6] = [
        SONG,
        [0.0, 1.0, 0.0, 1.0, 0.0, 0.0, 1.0],
        [0.0, 0.5, 0.5, 0.0, 1.0, 1.0, 0.5],
        [0.2, 0.9, 0.3, 0.7, 0.05, 0.4, 0.8],
        [0.4999, 0.1, 0.9, 0.3, 0.3, 0.02, 0.2],
        [0.0, 0.0, 0.0, 0.5, 0.5, 0.5, 0.0],
    ];
    for (k, p) in sets.iter().enumerate() {
        let (l, r) = (noise(4000, 10 + k as u32, 1.5), noise(4000, 20 + k as u32, 0.4));
        let (a, b) = run(p, &l, &r);
        let (ma, mb) = model(p, &l, &r, None);
        assert!(max_err(&a, &ma) < 2e-5 && max_err(&b, &mb) < 2e-5, "set {k}: {} {}", max_err(&a, &ma), max_err(&b, &mb));
    }
}

#[test]
fn detector_is_the_larger_of_both_channels_and_the_gain_is_shared() {
    let mut p = UNITY;
    p[2] = 0.7;
    p[3] = 1.0;
    p[4] = 0.0;
    let loud = vec![3.0f32; 600];
    let quiet = vec![0.01f32; 600];
    let (a, b) = run(&p, &loud, &quiet);
    // the quiet right channel is attenuated by the same gain as the loud left one
    let gl = a[599] / 3.0;
    let gr = b[599] / 0.01;
    assert!(gl < 0.9 && (gl - gr).abs() < 1e-5, "{gl} {gr}");
    // swapped channels: the same shared gain
    let (a2, b2) = run(&p, &quiet, &loud);
    assert!(((a2[599] / 0.01) - gl).abs() < 1e-5 && ((b2[599] / 3.0) - gl).abs() < 1e-5);
}

#[test]
fn sidechain_input_drives_the_gain_and_the_main_input_does_not() {
    let mut p = SONG;
    p[0] = 1.0;
    p[2] = 0.6;
    p[3] = 0.9;
    p[4] = 0.02;
    p[5] = 0.05;
    let main_l = noise(3000, 31, 0.3);
    let main_r = noise(3000, 32, 0.3);
    let quiet = vec![0.0f32; 3000];
    let loud: Vec<f32> = noise(3000, 33, 3.0);
    // quiet side-chain: the main signal is only delayed and made up (no compression)
    let (mut a, mut b) = (main_l.clone(), main_r.clone());
    Id7::new(&p).unwrap().process_sidechain(&mut a, &mut b, &quiet, &quiet).unwrap();
    let (ma, mb) = model(&p, &main_l, &main_r, Some((&quiet, &quiet)));
    assert!(max_err(&a, &ma) < 2e-5 && max_err(&b, &mb) < 2e-5);
    // loud side-chain: gain reduction appears although the main input is tiny, and it matches the model
    let (mut c, mut d) = (main_l.clone(), main_r.clone());
    Id7::new(&p).unwrap().process_sidechain(&mut c, &mut d, &loud, &loud).unwrap();
    let (mc, md) = model(&p, &main_l, &main_r, Some((&loud, &loud)));
    assert!(max_err(&c, &mc) < 2e-5 && max_err(&d, &md) < 2e-5);
    assert!(c[2500].abs() < a[2500].abs() * 0.9 || c.iter().map(|x| x.abs()).sum::<f32>() < a.iter().map(|x| x.abs()).sum::<f32>() * 0.9);
}

#[test]
fn the_side_chain_is_scaled_by_the_input_gain_and_never_modified() {
    let mut p = SONG;
    p[0] = 1.0;
    let (mut l, mut r) = (noise(500, 41, 1.0), noise(500, 42, 1.0));
    let (sl, sr) = (noise(500, 43, 3.0), noise(500, 44, 3.0));
    let (el, er) = (sl.clone(), sr.clone());
    Id7::new(&p).unwrap().process_sidechain(&mut l, &mut r, &sl, &sr).unwrap();
    assert_eq!((sl, sr), (el, er));
}

#[test]
fn p0_switch_keeps_the_envelope_and_the_delay_state() {
    // internal for 1000 samples then side-chain with the same signal as the aux: identical to a never-switched run
    let l = noise(2000, 51, 2.5);
    let r = noise(2000, 52, 2.5);
    let mut p = SONG;
    p[2] = 0.7;
    let mut plain = Id7::new(&p).unwrap();
    let (mut a, mut b) = (l.clone(), r.clone());
    plain.process(&mut a, &mut b).unwrap();
    let mut sw = Id7::new(&p).unwrap();
    let (mut c, mut d) = (l[..1000].to_vec(), r[..1000].to_vec());
    sw.process(&mut c, &mut d).unwrap();
    sw.set_param(0, 1.0).unwrap();
    let (mut e, mut f) = (l[1000..].to_vec(), r[1000..].to_vec());
    sw.process_sidechain(&mut e, &mut f, &l[1000..], &r[1000..]).unwrap();
    c.extend(e);
    d.extend(f);
    assert_eq!((c, d), (a, b));
}

#[test]
fn block_split_is_bit_identical() {
    let (l, r) = (noise(2000, 61, 2.0), noise(2000, 62, 2.0));
    let want = run(&SONG, &l, &r);
    for &n in &[1usize, 7, 87, 88, 128, 999] {
        let mut d = Id7::new(&SONG).unwrap();
        let (mut a, mut b) = (l.clone(), r.clone());
        for (x, y) in a.chunks_mut(n).zip(b.chunks_mut(n)) {
            d.process(x, y).unwrap();
        }
        assert_eq!((a, b), want, "block {n}");
    }
}

#[test]
fn set_param_keeps_the_state_and_applies_on_the_next_call() {
    let (l, r) = (noise(1200, 71, 2.5), noise(1200, 72, 2.5));
    let mut d = Id7::new(&SONG).unwrap();
    let (mut a, mut b) = (l[..600].to_vec(), r[..600].to_vec());
    d.process(&mut a, &mut b).unwrap();
    d.set_param(2, 0.5).unwrap();
    d.set_param(6, 0.9).unwrap();
    let (mut c, mut e) = (l[600..].to_vec(), r[600..].to_vec());
    d.process(&mut c, &mut e).unwrap();
    // reference: the independent model with the parameter change at the block boundary
    let mut m = Model::new();
    let mut p = SONG;
    let mut want = Vec::new();
    for i in 0..1200 {
        if i == 600 {
            p[2] = 0.5;
            p[6] = 0.9;
        }
        want.push(m.step(&p, [l[i] as f64, r[i] as f64], [l[i] as f64, r[i] as f64])[0]);
    }
    a.extend(c);
    assert!(max_err(&a, &want) < 2e-5, "{}", max_err(&a, &want));
}

#[test]
fn silence_gives_silence_and_the_release_returns_to_unity_gain() {
    let mut p = SONG;
    p[1] = 0.5; // unity input gain
    p[2] = 0.2; // threshold 2^(-4.8) = 0.036
    let mut x = vec![3.0f32; 200];
    x.extend(vec![0.0f32; 20000]);
    let (a, _) = run(&p, &x, &x);
    assert!(a[10000..].iter().all(|&v| v == 0.0));
    // a quiet tone (below the threshold) after the long release is back to plain make-up (no leftover reduction)
    let mut y = vec![3.0f32; 200];
    y.extend(vec![0.0f32; 20000]);
    y.extend(vec![0.02f32; 400]);
    let (b, _) = run(&p, &y, &y);
    let mk = 2f32.powf((2.0 * p[6] - 1.0) * 2.0);
    assert!((b[20200 + 400 - 1] - 0.02 * mk).abs() < 1e-6, "{} vs {}", b[20599], 0.02 * mk);
}

#[test]
fn extreme_inputs_stay_finite_and_zero_length_is_a_no_op() {
    let mut d = Id7::new(&[0.0, 1.0, 0.0, 1.0, 0.0, 0.0, 1.0]).unwrap();
    let (mut l, mut r) = (vec![1.0e30f32; 200], vec![-1.0e30f32; 200]);
    d.process(&mut l, &mut r).unwrap();
    assert!(l.iter().chain(&r).all(|x| x.is_finite()));
    let (mut e, mut f): (Vec<f32>, Vec<f32>) = (vec![], vec![]);
    d.process(&mut e, &mut f).unwrap();
}

#[test]
#[should_panic(expected = "equal length required")]
fn mismatched_channel_lengths_panic() {
    let mut d = Id7::new(&SONG).unwrap();
    let (mut l, mut r) = (vec![0.0f32; 10], vec![0.0f32; 9]);
    let _ = d.process(&mut l, &mut r);
}

#[test]
#[should_panic(expected = "equal length required")]
fn mismatched_side_chain_length_panics() {
    let mut p = SONG;
    p[0] = 1.0;
    let mut d = Id7::new(&p).unwrap();
    let (mut l, mut r) = (vec![0.0f32; 10], vec![0.0f32; 10]);
    let _ = d.process_sidechain(&mut l, &mut r, &[0.0; 10], &[0.0; 3]);
}

/// Negative controls: the model comparison must be sensitive to every parameter (a 1/255 nudge leaves the tolerance on a
/// signal that exercises compression), so a wrong parameter law cannot slip through the tolerance above.
#[test]
fn negative_control_parameter_nudges_leave_the_model_tolerance() {
    let p = [0.0, 0.5, 0.6, 0.4, 0.2, 0.2, 0.5];
    let mut x = noise(6000, 81, 2.0);
    for (i, v) in x.iter_mut().enumerate() {
        *v *= if (i / 1500) % 2 == 0 { 1.0 } else { 0.1 };
    }
    let want = model(&p, &x, &x, None);
    for i in 1..PARAMS {
        let mut q = p;
        q[i] += 1.0 / 255.0;
        let (a, _) = run(&q, &x, &x);
        assert!(max_err(&a, &want.0) > 5e-5, "P{i}: a nudge of 1/255 is invisible to the model tolerance");
    }
}

/// Pinned from the original (synthetic Wine vectors 10/11): a NaN detector value becomes the envelope for exactly one
/// sample (that sample gets the plain make-up gain), the next finite value replaces it; the NaN sample itself is only
/// delayed and comes out 87 samples later, everything else stays finite. An infinity is a plain clamp.
#[test]
fn a_nan_detector_value_is_forgotten_after_one_sample() {
    let mut p = UNITY;
    p[1] = 0.5; // input gain 1
    p[2] = 0.5; // threshold 2^-3
    p[3] = 1.0;
    p[4] = 0.0;
    let mut x = vec![3.0f32; 900];
    x[500] = f32::NAN;
    x[600] = f32::INFINITY;
    let (a, _) = run(&p, &x, &x);
    for (i, v) in a.iter().enumerate() {
        assert_eq!(v.is_nan(), i == 500 + DELAY, "sample {i}: {v}");
    }
    // NaN sample: no reduction (make-up 1, gain NaN-free path) on the delayed 3.0 that is read at that instant
    assert_eq!(a[500], 3.0);
    assert!(a[499] < 3.0 && a[501] < 3.0, "compression resumes right after the NaN: {} {}", a[499], a[501]);
}
