//! Synthetic tests only for `id35` (second-order all-pass cascade). Expectations come from an independent f64 model
//! written from the behaviour notes plus signal-theory properties of all-pass filters (unit magnitude, −π phase at
//! the centre frequency), never from third-party data.
use crate::id35::{Error, Id35, PARAMS};

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

fn run(p: &[f32; PARAMS], l: &[f32], r: &[f32]) -> (Vec<f32>, Vec<f32>) {
    let mut d = Id35::new(p).expect("supported");
    let (mut a, mut b) = (l.to_vec(), r.to_vec());
    d.process(&mut a, &mut b);
    (a, b)
}

/// Stage count `trunc(p3·256)` (f32 product, as the device).
fn stages(p3: f32) -> usize {
    (p3 * 256.0) as usize
}

/// Per-stage centre frequency in Hz: independent restatement of the observed rule. Stage `i` hashes `i·1234567`
/// (32-bit integer finaliser) to a uniform `u ∈ [−1, 1)`, scales by `3·P4` octaves and the base `P0²·19980 + 20`,
/// clamped to `[20, 20000]`.
fn stage_hz(p: &[f32; PARAMS], i: usize) -> f64 {
    let mut h = (i as u32).wrapping_mul(1_234_567);
    h = (h ^ (h >> 16)).wrapping_mul(0x7feb_352d);
    h = (h ^ (h >> 15)).wrapping_mul(0x846c_a68b);
    h ^= h >> 16;
    let u = h as f64 / 2_147_483_648.0 - 1.0;
    let base = (p[0] as f64).powi(2) * 19980.0 + 20.0;
    (base * (2.0f64).powf(u * 3.0 * p[4] as f64)).clamp(20.0, 20000.0)
}

/// Independent f64 model: one channel, direct form II transposed-free textbook second-order all-pass per stage with
/// persistent state per stage (states of stages above the live count are untouched). `Q = max(0.5 + 19.5·P1², 0.5)`.
struct Model {
    /// `[u, v]` per stage, 256 stages.
    st: Vec<[f64; 2]>,
}

impl Model {
    fn new() -> Self {
        Model { st: vec![[0.0; 2]; 256] }
    }

    fn step(&mut self, p: &[f32; PARAMS], x: f64) -> f64 {
        let q = ((p[1] as f64).powi(2) * 19.5 + 0.5).max(0.5);
        let mut v = x;
        for i in 0..stages(p[3]) {
            let w = 2.0 * core::f64::consts::PI * stage_hz(p, i) / FS;
            let a = w.sin() * 0.5 / q;
            let (c1, c2) = (-2.0 * w.cos() / (1.0 + a), (1.0 - a) / (1.0 + a));
            let [u, vv] = self.st[i];
            let wn = v - c1 * u - c2 * vv;
            let y = c2 * wn + c1 * u + vv;
            self.st[i] = [wn, u];
            v = y;
        }
        (v - x) * p[2] as f64 + x
    }
}

fn model(p: &[f32; PARAMS], x: &[f32]) -> Vec<f64> {
    let mut m = Model::new();
    x.iter().map(|&v| m.step(p, v as f64)).collect()
}

fn max_err(got: &[f32], want: &[f64]) -> f64 {
    got.iter().zip(want).map(|(&g, &w)| (g as f64 - w).abs()).fold(0.0, f64::max)
}

/// Song-like base chunk: second-order topology (P5 = 1), 12 stages.
const P: [f32; PARAMS] = [0.3, 0.4, 1.0, 12.5 / 256.0, 0.5, 1.0];

#[test]
fn rejects_bad_values_explicitly() {
    for i in 0..PARAMS {
        for v in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, -0.001, 1.001] {
            let mut d = Id35::new(&P).unwrap();
            assert_eq!(d.set_param(i, v), Err(Error::Range), "set_param({i}, {v})");
            let mut p = P;
            p[i] = v;
            assert_eq!(Id35::new(&p).err(), Some(Error::Range), "new with p[{i}]={v}");
        }
    }
    let mut d = Id35::new(&P).unwrap();
    assert_eq!(d.set_param(PARAMS, 0.5), Err(Error::Index));
    assert_eq!(d.set_param(usize::MAX, 0.5), Err(Error::Index));
}

#[test]
fn topology_other_than_second_order_is_rejected() {
    // trunc(P5·2.999): 0 = delay-line all-pass, 1 = first-order, 2 = second-order (the only one supported)
    for v in [0.0, 0.1, 0.3333, 0.5, 0.66, 0.6668] {
        let mut d = Id35::new(&P).unwrap();
        assert_eq!(d.set_param(5, v), Err(Error::Topology), "P5={v}");
        let mut p = P;
        p[5] = v;
        assert_eq!(Id35::new(&p).err(), Some(Error::Topology), "new P5={v}");
    }
    for v in [0.667, 0.7, 0.9, 1.0] {
        assert_eq!(Id35::new(&[P[0], P[1], P[2], P[3], P[4], v]).err(), None, "P5={v}");
    }
}

#[test]
fn accepts_the_full_unit_range_of_the_live_parameters() {
    for v in [0.0, 1.0, 0.5, 0.001, 0.999, 128.0 / 255.0] {
        for i in 0..5 {
            let mut d = Id35::new(&P).unwrap();
            assert_eq!(d.set_param(i, v), Ok(()), "P{i}={v}");
            let (mut l, mut r) = (noise(300, 5), noise(300, 6));
            d.process(&mut l, &mut r);
            assert!(l.iter().chain(&r).all(|x| x.is_finite()), "P{i}={v}");
        }
    }
}

#[test]
fn rejection_leaves_the_device_untouched() {
    let (l, r) = (noise(300, 1), noise(300, 2));
    let want = run(&P, &l, &r);
    let mut d = Id35::new(&P).unwrap();
    for (i, v) in [(3, f32::NAN), (3, 1.5), (0, -1.0), (5, 0.1), (5, 0.0), (6, 0.5), (9, 0.5)] {
        assert!(d.set_param(i, v).is_err());
    }
    let (mut a, mut b) = (l.clone(), r.clone());
    d.process(&mut a, &mut b);
    assert_eq!((a, b), want);
}

#[test]
fn p5_values_selecting_the_same_topology_are_bit_identical() {
    let (l, r) = (noise(512, 3), noise(512, 4));
    let want = run(&P, &l, &r);
    for v in [0.667, 0.8, 0.999] {
        let mut p = P;
        p[5] = v;
        assert_eq!(run(&p, &l, &r), want, "P5={v}");
    }
}

#[test]
fn mix_zero_is_the_exact_dry_signal_and_zero_stages_is_identity() {
    let (l, r) = (noise(512, 7), noise(512, 8));
    let mut p = P;
    p[2] = 0.0;
    assert_eq!(run(&p, &l, &r), (l.clone(), r.clone()));
    let mut q = P;
    q[3] = 0.0;
    q[2] = 0.7;
    assert_eq!(run(&q, &l, &r), (l, r));
}

#[test]
fn mix_is_a_linear_crossfade_of_the_all_pass_output() {
    let (l, r) = (noise(512, 9), noise(512, 10));
    let wet = run(&P, &l, &r);
    for m in [0.25f32, 0.5, 0.8] {
        let mut p = P;
        p[2] = m;
        let got = run(&p, &l, &r);
        for i in 0..l.len() {
            assert_eq!(got.0[i], (wet.0[i] - l[i]) * m + l[i], "L {i} mix {m}");
            assert_eq!(got.1[i], (wet.1[i] - r[i]) * m + r[i], "R {i} mix {m}");
        }
    }
}

#[test]
fn matches_the_independent_f64_model() {
    let (l, r) = (noise(1024, 11), noise(1024, 12));
    let cases: [[f32; PARAMS]; 5] = [
        [0.3, 0.4, 1.0, 1.5 / 256.0, 0.0, 1.0],
        [0.5, 0.0, 1.0, 8.5 / 256.0, 0.0, 1.0],
        [0.7, 0.8, 0.6, 20.5 / 256.0, 0.5, 1.0],
        [0.15, 0.9, 1.0, 40.5 / 256.0, 1.0, 0.7],
        [0.9, 0.2, 0.35, 30.5 / 256.0, 0.2, 0.9],
    ];
    for p in cases {
        let (a, b) = run(&p, &l, &r);
        let (ma, mb) = (model(&p, &l), model(&p, &r));
        let e = max_err(&a, &ma).max(max_err(&b, &mb));
        // f32 vs f64: the high-Q, low-frequency, strongly scattered case is the worst conditioned (~3e-4)
        assert!(e < 1e-3, "{p:?}: max err {e}");
        assert!(e > 0.0 || p[2] == 0.0, "{p:?}: suspiciously exact");
    }
}

/// Full 256-stage cascade (P3 = 1.0 -> `MAX_STAGES`, the song's regime of narrow scattered sections), ungated.
#[test]
fn matches_the_independent_f64_model_at_256_stages() {
    assert_eq!(stages(1.0), 256);
    let (l, r) = (noise(2048, 21), noise(2048, 22));
    for p in [[0.5, 0.3, 1.0, 1.0, 0.5, 1.0], [0.3, 0.6, 0.7, 1.0, 1.0, 0.9], [0.15, 0.9, 1.0, 1.0, 1.0, 0.7]] {
        let (a, b) = run(&p, &l, &r);
        let (ma, mb) = (model(&p, &l), model(&p, &r));
        let e = max_err(&a, &ma).max(max_err(&b, &mb));
        eprintln!("{p:?}: 256 stages max err {e:e}");
        // measured 2.6e-5 / 4.5e-5 / 4.2e-4 (the narrow high-Q low-frequency case); 1e-3 as in the shorter cascades
        assert!(e < 1e-3, "{p:?}: max err {e}");
        assert!(e > 0.0, "{p:?}: suspiciously exact");
    }
}

#[test]
fn all_pass_preserves_energy() {
    // full wet, 255 stages worth of phase, the tail rung out with silence: energy in == energy out
    let mut p = P;
    p[0] = 0.5;
    p[1] = 0.1;
    p[3] = 255.5 / 256.0;
    let mut d = Id35::new(&p).unwrap();
    let n = 4096;
    let mut l = noise(n, 13);
    l.extend(core::iter::repeat(0.0).take(60_000));
    let mut r = l.clone();
    let e_in: f64 = l.iter().map(|&x| x as f64 * x as f64).sum();
    d.process(&mut l, &mut r);
    let e_out: f64 = l.iter().map(|&x| x as f64 * x as f64).sum();
    assert!((e_out / e_in - 1.0).abs() < 0.02, "energy ratio {}", e_out / e_in);
    assert_eq!(l, r, "identical channels give identical outputs");
}

#[test]
fn centre_frequency_gets_minus_pi_per_stage() {
    // equal stage frequencies (P4 = 0): each second-order all-pass is exactly −π (inverted) at its centre
    for (n, sign) in [(1usize, -1.0f64), (2, 1.0), (3, -1.0)] {
        let p = [0.3, 0.0, 1.0, (n as f32 + 0.5) / 256.0, 0.0, 1.0];
        let hz = 0.3f64 * 0.3 * 19980.0 + 20.0;
        let x: Vec<f32> = (0..6000).map(|i| (0.5 * (2.0 * core::f64::consts::PI * hz * i as f64 / FS).sin()) as f32).collect();
        let (y, _) = run(&p, &x, &x);
        let e = (4000..6000).map(|i| (y[i] as f64 - sign * x[i] as f64).abs()).fold(0.0, f64::max);
        assert!(e < 2e-3, "n={n}: {e}");
    }
}

#[test]
fn channels_are_independent() {
    let (l, r) = (noise(700, 14), noise(700, 15));
    let (a, b) = run(&P, &l, &r);
    let (b2, a2) = run(&P, &r, &l);
    assert_eq!((a, b), (a2, b2));
    let silent = vec![0.0f32; 700];
    let (_, rr) = run(&P, &l, &silent);
    assert!(rr.iter().all(|&x| x == 0.0));
}

#[test]
fn block_splits_do_not_change_the_output() {
    let (l, r) = (noise(1000, 16), noise(1000, 17));
    let want = run(&P, &l, &r);
    let (mut a, mut b) = (l.clone(), r.clone());
    let mut d = Id35::new(&P).unwrap();
    let mut at = 0;
    for len in [1, 127, 128, 3, 256, 5, 480] {
        d.process(&mut a[at..at + len], &mut b[at..at + len]);
        at += len;
    }
    assert_eq!(at, 1000);
    assert_eq!((a, b), want);
}

#[test]
fn params_apply_to_the_next_call_and_state_is_kept() {
    let (l, r) = (noise(600, 18), noise(600, 19));
    let mut d = Id35::new(&P).unwrap();
    let (mut a, mut b) = (l.clone(), r.clone());
    d.process(&mut a[..300], &mut b[..300]);
    d.set_param(0, 0.8).unwrap();
    d.set_param(4, 0.1).unwrap();
    d.process(&mut a[300..], &mut b[300..]);
    // model: same states carried over, new parameters for the second half
    let mut p2 = P;
    p2[0] = 0.8;
    p2[4] = 0.1;
    let mut m = Model::new();
    let want: Vec<f64> = l.iter().enumerate().map(|(i, &x)| m.step(if i < 300 { &P } else { &p2 }, x as f64)).collect();
    assert!(max_err(&a, &want) < 2e-4);
}

#[test]
fn stage_states_persist_while_the_count_is_lowered_and_raised() {
    // The device never clears stage state: stages above the live count keep their history and resume from it.
    let l = noise(900, 20);
    let (c_hi, c_lo) = (60.5 / 256.0, 5.5 / 256.0);
    let mut p = P;
    p[3] = c_hi;
    let mut d = Id35::new(&p).unwrap();
    let mut m = Model::new();
    let mut a = l.clone();
    let mut r = l.clone();
    let mut want = Vec::new();
    for (k, chunk) in l.chunks(300).enumerate() {
        let c = [c_hi, c_lo, c_hi][k];
        p[3] = c;
        d.set_param(3, c).unwrap();
        want.extend(chunk.iter().map(|&x| m.step(&p, x as f64)));
        let (s, e) = (k * 300, k * 300 + chunk.len());
        d.process(&mut a[s..e], &mut r[s..e]);
    }
    assert!(max_err(&a, &want) < 5e-4, "{}", max_err(&a, &want));
}

#[test]
fn silence_in_silence_out_and_the_ringing_decays() {
    let mut d = Id35::new(&P).unwrap();
    let (mut a, mut b) = (vec![0.0f32; 400], vec![0.0f32; 400]);
    d.process(&mut a, &mut b);
    assert!(a.iter().chain(&b).all(|&x| x == 0.0));
    let (mut l, mut r) = (noise(200, 21), noise(200, 22));
    d.process(&mut l, &mut r);
    let (mut z, mut zr) = (vec![0.0f32; 200_000], vec![0.0f32; 200_000]);
    d.process(&mut z, &mut zr);
    // the ring-out ends in a sub-1e-37 rounding limit cycle just above the flush-to-zero threshold (the original's
    // arithmetic does the same): inaudible, but not exactly zero
    assert!(z[199_000..].iter().chain(&zr[199_000..]).all(|&x| x.abs() < 1e-30), "ringing decayed");
}

#[test]
#[should_panic(expected = "channel length mismatch")]
fn channel_length_mismatch_panics() {
    let mut d = Id35::new(&P).unwrap();
    d.process(&mut [0.0; 4], &mut [0.0; 3]);
}
