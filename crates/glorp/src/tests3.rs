//! Synthetic tests only for `id3` (waveshaper / saturator). Expectations come from closed-form values, from an independent
//! f64 model written from the behaviour notes (research/glorp-devices.md §ID 3) and from properties (block-split identity,
//! channel independence, inert parameters), never from third-party data.
use crate::id3::{Error, Id3, PARAMS};

fn noise(n: usize, seed: u32, amp: f32) -> Vec<f32> {
    let mut s = seed;
    (0..n)
        .map(|_| {
            s = s.wrapping_mul(1664525).wrapping_add(1013904223);
            ((s >> 8) as f32 / (1u32 << 24) as f32 - 0.5) * 2.0 * amp
        })
        .collect()
}

/// The song's own settings: mode 3 (soft knee), 4x oversampling, ceiling 1, threshold 1, width 0.645, slope 0.
const SONG: [f32; PARAMS] = [0.0, 1.0, 0.0, 1.0, 1.0, 1.0, 0.715, 0.655, 1.0, 0.645, 0.0];
/// Drive 1 (P0 = 0.2), ceiling 1, no fold, mix 1, mode 0, no oversampling, unity gains (P6 = P7 = 0.5), the rest mid.
const FLAT: [f32; PARAMS] = [0.2, 1.0, 0.0, 1.0, 0.0, 0.0, 0.5, 0.5, 1.0, 0.5, 0.5];

fn with(base: &[f32; PARAMS], set: &[(usize, f32)]) -> [f32; PARAMS] {
    let mut p = *base;
    for &(i, v) in set {
        p[i] = v;
    }
    p
}

fn run(p: &[f32; PARAMS], l: &[f32], r: &[f32]) -> (Vec<f32>, Vec<f32>) {
    let mut d = Id3::new(p).expect("supported");
    let (mut a, mut b) = (l.to_vec(), r.to_vec());
    d.process(&mut a, &mut b);
    (a, b)
}

fn mono(p: &[f32; PARAMS], x: &[f32]) -> Vec<f32> {
    run(p, x, &vec![0.0; x.len()]).0
}

/// Independent f64 model, **finite inputs only**: f64 `max`/`min`/`clamp` and the f32 device treat NaN differently (the device
/// clamps NaN to −1), so NaN / inf behaviour is not claimed here; it is pinned by `nan_and_infinite_inputs_are_pinned` and
/// checked against the original's recorded vectors in `tests/reference3.rs`. `mode` 0..=3, `over` 0 (none), 1 (2x), 2 (4x) are passed explicitly so the boundary tests can
/// check the device's own parameter decoding against the intended value.
struct Model {
    p: [f64; PARAMS],
    mode: usize,
    over: usize,
    prev: f64,
}

impl Model {
    fn new(p: &[f32; PARAMS]) -> Self {
        let q: Vec<f64> = p.iter().map(|&v| v as f64).collect();
        Model { p: q.clone().try_into().unwrap(), mode: ((p[4] * 3.999) as usize), over: ((p[5] * 2.0) as usize), prev: 0.0 }
    }

    fn drive(&self) -> f64 {
        let u = self.p[0];
        if u < 0.2 { u + 0.8 } else { 5.0 * (5.0 * u - 1.0).powi(2) + 1.0 }
    }

    fn gain(u: f64) -> f64 {
        if u > 0.5 { 4.0 * u - 1.5 } else { 2.0 * u }
    }

    fn shape(&self, x: f64, k: f64) -> f64 {
        let c = self.p[1];
        let mut y = x * k / c;
        match self.mode {
            0 => {
                let f = self.p[2];
                if f > 0.0 && y < -1.0 {
                    y = f * (-1.0 - y) - 1.0;
                } else if f > 0.0 && y > 1.0 {
                    y = f * (1.0 - y) + 1.0;
                }
            }
            1 => y *= std::f64::consts::FRAC_PI_2,
            2 => y = y * y * y,
            _ => {
                let (thr, w, slope) = (self.p[8], self.p[9], self.p[10]);
                let a = y.abs();
                let t = a - thr;
                let b = if 2.0 * t < -w {
                    a
                } else if 2.0 * t > w {
                    thr + slope * t
                } else {
                    a + (t + w / 2.0).powi(2) * (slope - 1.0) / (2.0 * w)
                };
                y = b.copysign(y);
            }
        }
        y.clamp(-1.0, 1.0) * c
    }

    fn step(&mut self, x: f64) -> f64 {
        let k = self.drive();
        let y0 = self.shape(x * Self::gain(self.p[6]), k) * Self::gain(self.p[7]);
        let y = match self.over {
            0 => y0,
            1 => (self.shape((self.prev + x) * 0.5, k) + y0) * 0.5,
            _ => {
                let m = (self.prev + x) * 0.5;
                let (q0, q2) = ((self.prev + m) * 0.5, (m + x) * 0.5);
                (y0 + self.shape(m, k) + self.shape(q0, k) + self.shape(q2, k)) * 0.25
            }
        };
        self.prev = x;
        x + (y - x) * self.p[3]
    }
}

fn model(p: &[f32; PARAMS], x: &[f32]) -> Vec<f64> {
    let mut m = Model::new(p);
    x.iter().map(|&v| m.step(v as f64)).collect()
}

fn max_err(got: &[f32], want: &[f64]) -> f64 {
    got.iter().zip(want).map(|(&g, &w)| (g as f64 - w).abs()).fold(0.0, f64::max)
}

#[test]
fn rejects_bad_values_and_indices_explicitly() {
    for i in 0..PARAMS {
        for v in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, -0.001, 1.001] {
            let mut d = Id3::new(&SONG).unwrap();
            assert_eq!(d.set_param(i, v), Err(Error::Range), "set_param({i}, {v})");
            let mut p = SONG;
            p[i] = v;
            assert_eq!(Id3::new(&p).err(), Some(Error::Range), "new with p[{i}]={v}");
        }
    }
    let mut d = Id3::new(&SONG).unwrap();
    // the original silently ignores index >= 11; here that is an error
    assert_eq!(d.set_param(PARAMS, 0.5), Err(Error::Index));
    assert_eq!(d.set_param(usize::MAX, 0.5), Err(Error::Index));
}

#[test]
fn rejection_leaves_the_device_untouched() {
    let (l, r) = (noise(600, 1, 2.0), noise(600, 2, 2.0));
    let want = run(&SONG, &l, &r);
    let mut d = Id3::new(&SONG).unwrap();
    for (i, v) in [(3, f32::NAN), (3, 1.5), (1, -1.0), (11, 0.5), (20, 0.5), (0, 2.0)] {
        assert!(d.set_param(i, v).is_err());
    }
    let (mut a, mut b) = (l.clone(), r.clone());
    d.process(&mut a, &mut b);
    assert_eq!((a, b), want);
}

#[test]
fn accepts_the_whole_unit_range_of_every_parameter() {
    for v in [0.0, 1.0, 0.5, 0.4999, 0.001, 0.999, 128.0 / 255.0] {
        for i in 0..PARAMS {
            let mut d = Id3::new(&SONG).unwrap();
            assert_eq!(d.set_param(i, v), Ok(()), "P{i}={v}");
        }
    }
    for v in [0.0, 0.2, 0.5, 1.0] {
        for i in (0..PARAMS).filter(|&i| i != 1 && i != 9) {
            let mut d = Id3::new(&SONG).unwrap();
            d.set_param(i, v).unwrap();
            let (mut l, mut r) = (noise(500, 5, 3.0), noise(500, 6, 3.0));
            d.process(&mut l, &mut r);
            assert!(l.iter().chain(&r).all(|x| x.is_finite()), "P{i}={v}");
        }
    }
}

/// `trunc(P4 · 3.999)` selects the mode, `trunc(2 · P5)` the oversampling: the quarter / half boundaries are exact in f32.
#[test]
fn mode_and_oversampling_boundaries_follow_the_truncation() {
    let x: Vec<f32> = (0..200).map(|i| (i as f32 * 0.09 - 8.0) * 0.5).collect();
    // sensitive settings: fold 1, knee at 0.3 / width 0.2 / slope 0.2, ceiling 0.8, drive 2
    let base = with(&FLAT, &[(0, 0.3), (1, 0.8), (2, 1.0), (8, 0.3), (9, 0.2), (10, 0.2)]);
    for (p4, mode) in [(0.0, 0), (0.2500, 0), (0.2501, 1), (0.4999, 1), (0.5, 1), (0.5004, 2), (0.7501, 2), (0.7503, 3), (1.0, 3)] {
        let p = with(&base, &[(4, p4)]);
        let mut m = Model::new(&p);
        m.mode = mode;
        let want: Vec<f64> = x.iter().map(|&v| m.step(v as f64)).collect();
        assert!(max_err(&mono(&p, &x), &want) < 1e-5, "P4 = {p4} must select mode {mode}");
    }
    for (p5, over) in [(0.0, 0), (0.4999, 0), (0.5, 1), (0.9999, 1), (1.0, 2)] {
        let p = with(&base, &[(5, p5)]);
        let mut m = Model::new(&p);
        m.over = over;
        let want: Vec<f64> = x.iter().map(|&v| m.step(v as f64)).collect();
        assert!(max_err(&mono(&p, &x), &want) < 1e-5, "P5 = {p5} must select oversampling {over}");
    }
}

#[test]
fn mode0_hard_clip_and_fold_closed_form() {
    let x = [0.3, 1.5, -3.0, 2.0, -1.0, 0.0, 1.0];
    // fold 0: plain hard clip at ±1
    assert_eq!(mono(&FLAT, &x), vec![0.3, 1.0, -1.0, 1.0, -1.0, 0.0, 1.0]);
    // fold f reflects the overshoot with factor f: y = f·(b − y) + b
    let h = mono(&with(&FLAT, &[(2, 0.5)]), &x);
    for (g, w) in h.iter().zip([0.3, 0.75, 0.0, 0.5, -1.0, 0.0, 1.0]) {
        assert!((g - w).abs() < 1e-6, "{h:?}");
    }
    // 1.5 → 0.5, -3 → -1 + 1·(-1+3)... full reflection is clamped after the fold: 2 → 0, -3 → 1 (then clamped)
    let f = mono(&with(&FLAT, &[(2, 1.0)]), &x);
    for (g, w) in f.iter().zip([0.3, 0.5, 1.0, 0.0, -1.0, 0.0, 1.0]) {
        assert!((g - w).abs() < 1e-6, "{f:?}");
    }
}

#[test]
fn mode1_is_pi_half_gain_with_hard_clip_and_mode2_is_cubic() {
    let m1 = with(&FLAT, &[(4, 0.3)]);
    let o = mono(&m1, &[0.3, 0.5, 0.7, -0.2, -0.7]);
    for (g, w) in o.iter().zip([0.3 * 1.570796, 0.5 * 1.570796, 1.0, -0.2 * 1.570796, -1.0]) {
        assert!((g - w).abs() < 1e-6, "{o:?}");
    }
    let m2 = with(&FLAT, &[(4, 0.6)]);
    let o = mono(&m2, &[0.5, -0.5, 0.9, 2.0, -3.0, 1.0]);
    for (g, w) in o.iter().zip([0.125, -0.125, 0.729, 1.0, -1.0, 1.0]) {
        assert!((g - w).abs() < 1e-6, "{o:?}");
    }
}

/// Soft knee: below `thr − w/2` untouched, above `thr + w/2` a line of slope P10 through the threshold, quadratic between.
#[test]
fn mode3_soft_knee_closed_form_and_symmetry() {
    let p = with(&FLAT, &[(4, 0.9), (8, 0.5), (9, 0.4), (10, 0.25)]);
    let x = [0.2, 0.3, 0.6, 0.9, 1.0, 0.0];
    let pos = mono(&p, &x);
    // x = 0.6: t = 0.1 in the knee: 0.6 + (0.1 + 0.2)^2 · (0.25 − 1)/0.8 = 0.515625
    // x = 0.9: t = 0.4 above: 0.5 + 0.25·0.4 = 0.6; x = 1: t = 0.5: 0.625
    for (g, w) in pos.iter().zip([0.2, 0.3, 0.515625, 0.6, 0.625, 0.0]) {
        assert!((g - w).abs() < 1e-6, "{pos:?}");
    }
    let neg: Vec<f32> = x.iter().map(|v| -v).collect();
    let n = mono(&p, &neg);
    for (a, b) in pos.iter().zip(&n) {
        assert!((a + b).abs() < 1e-7, "odd symmetry: {a} {b}");
    }
    // slope 1 = no shaping above the knee start, slope 0 = a ceiling at the threshold
    let flat = mono(&with(&p, &[(10, 0.0)]), &[5.0, -5.0, 0.7]);
    assert!((flat[0] - 0.5).abs() < 1e-6 && (flat[1] + 0.5).abs() < 1e-6 && (flat[2] - 0.5).abs() < 0.05, "{flat:?}");
}

#[test]
fn ceiling_drive_gain_and_mix_laws() {
    // ceiling P1 = 0.5 scales the clip level: output peak 0.5
    let o = mono(&with(&FLAT, &[(1, 0.5)]), &[0.2, 0.9, 3.0]);
    assert!((o[0] - 0.2).abs() < 1e-6 && (o[1] - 0.5).abs() < 1e-6 && (o[2] - 0.5).abs() < 1e-6, "{o:?}");
    // drive: u < 0.2 → u + 0.8, else 5(5u − 1)^2 + 1 (continuous at 0.2); linear region, x = 0.1
    for (u, k) in [(0.0f32, 0.8f32), (0.1, 0.9), (0.2, 1.0), (0.3, 2.25), (0.4, 6.0)] {
        let o = mono(&with(&FLAT, &[(0, u)]), &[0.01]);
        assert!((o[0] - 0.01 * k).abs() < 1e-6, "P0 = {u}: {o:?}");
    }
    // input gain P6 and output gain P7: u ≤ 0.5 → 2u, else 4u − 1.5 (linear region)
    for (u, g) in [(0.0f32, 0.0f32), (0.25, 0.5), (0.5, 1.0), (0.75, 1.5), (1.0, 2.5)] {
        let a = mono(&with(&FLAT, &[(6, u)]), &[0.1]);
        let b = mono(&with(&FLAT, &[(7, u)]), &[0.1]);
        assert!((a[0] - 0.1 * g).abs() < 1e-6 && (b[0] - 0.1 * g).abs() < 1e-6, "u = {u}: {a:?} {b:?}");
    }
    // mix: 0 = exact dry, 1 = wet, 0.5 = halfway
    let x = noise(300, 9, 3.0);
    let hot = with(&FLAT, &[(4, 0.9), (8, 0.3), (0, 0.5)]);
    assert_eq!(mono(&with(&hot, &[(3, 0.0)]), &x), x);
    let wet = mono(&with(&hot, &[(3, 1.0)]), &x);
    let half = mono(&with(&hot, &[(3, 0.5)]), &x);
    for i in 0..x.len() {
        assert!((half[i] - (x[i] + wet[i]) * 0.5).abs() < 1e-6, "{i}");
    }
}

/// The oversampled paths in the linear region (|·| ≪ ceiling): the sub-samples are raw (no input / output gain), so the
/// result is a short average of the current and previous input; closed forms from the notes.
#[test]
fn oversampling_in_the_linear_region_is_a_short_average() {
    let x = [0.01f32, 0.04, -0.02, 0.03, 0.0, -0.05];
    let two = mono(&with(&FLAT, &[(5, 0.5)]), &x);
    let four = mono(&with(&FLAT, &[(5, 1.0)]), &x);
    let mut prev = 0.0f32;
    for (i, &c) in x.iter().enumerate() {
        let m = (prev + c) * 0.5;
        let w2 = (m + c) * 0.5;
        let w4 = (c + m + (prev + m) * 0.5 + (m + c) * 0.5) * 0.25;
        assert!((two[i] - w2).abs() < 1e-7, "2x sample {i}: {} vs {w2}", two[i]);
        assert!((four[i] - w4).abs() < 1e-7, "4x sample {i}: {} vs {w4}", four[i]);
        prev = c;
    }
}

#[test]
fn matches_an_independent_f64_model_on_every_mode_and_oversampling() {
    let x = noise(3000, 21, 3.0);
    let sets: [[(usize, f32); 6]; 8] = [
        [(4, 0.0), (5, 0.0), (0, 0.25), (2, 0.0), (1, 1.0), (3, 1.0)],
        [(4, 0.1), (5, 0.5), (0, 0.4), (2, 0.6), (1, 0.7), (3, 0.8)],
        [(4, 0.3), (5, 0.0), (0, 0.1), (6, 0.8), (7, 0.3), (3, 1.0)],
        [(4, 0.3), (5, 1.0), (0, 0.6), (6, 0.3), (7, 0.9), (3, 0.6)],
        [(4, 0.6), (5, 0.0), (0, 0.3), (1, 0.9), (6, 0.6), (7, 0.6)],
        [(4, 0.6), (5, 0.9), (0, 0.5), (1, 0.4), (3, 0.9), (2, 0.5)],
        [(4, 0.9), (5, 0.0), (8, 0.4), (9, 0.5), (10, 0.3), (0, 0.45)],
        [(4, 0.9), (5, 1.0), (8, 0.7), (9, 0.2), (10, 0.9), (1, 0.8)],
    ];
    for (n, s) in sets.iter().enumerate() {
        let p = with(&FLAT, s);
        let got = mono(&p, &x);
        let e = max_err(&got, &model(&p, &x));
        assert!(e < 1e-4, "set {n}: max error {e}");
    }
    let e = max_err(&mono(&SONG, &x), &model(&SONG, &x));
    assert!(e < 1e-4, "song settings: max error {e}");
}

#[test]
fn per_block_automation_matches_the_model_with_state_kept() {
    let x = noise(64 * 40, 33, 3.0);
    let mut d = Id3::new(&SONG).unwrap();
    let mut m = Model::new(&SONG);
    let mut got = Vec::new();
    let mut want = Vec::new();
    let mut p = SONG;
    for b in 0..40 {
        // sweep modes / oversampling / knee every block
        let t = b as f32 / 40.0;
        for (i, v) in [(4, (t * 5.0).fract()), (5, (t * 3.0).fract()), (0, 0.2 + 0.6 * t), (9, 0.2 + 0.5 * t), (3, 1.0 - 0.5 * t)] {
            d.set_param(i, v).unwrap();
            p[i] = v;
        }
        let prev = m.prev;
        m = Model::new(&p);
        m.prev = prev;
        let mut blk = x[b * 64..(b + 1) * 64].to_vec();
        let mut z = vec![0.0; 64];
        d.process(&mut blk, &mut z);
        got.extend(blk);
        want.extend(x[b * 64..(b + 1) * 64].iter().map(|&v| m.step(v as f64)));
    }
    assert!(max_err(&got, &want) < 1e-4);
}

#[test]
fn block_split_is_bit_identical_and_channels_are_independent() {
    let (l, r) = (noise(1000, 41, 3.0), noise(1000, 42, 3.0));
    let whole = run(&SONG, &l, &r);
    let mut d = Id3::new(&SONG).unwrap();
    let (mut a, mut b) = (l.clone(), r.clone());
    for (s, e) in [(0, 1), (1, 130), (130, 131), (131, 700), (700, 1000)] {
        d.process(&mut a[s..e], &mut b[s..e]);
    }
    assert_eq!((a, b), whole);
    // R content does not influence L and vice versa
    let (l2, _) = run(&SONG, &l, &noise(1000, 99, 5.0));
    assert_eq!(l2, whole.0);
    let (_, r2) = run(&SONG, &noise(1000, 98, 5.0), &r);
    assert_eq!(r2, whole.1);
}

#[test]
fn set_param_keeps_the_oversampling_history() {
    let x = noise(400, 51, 3.0);
    let p = with(&SONG, &[(5, 1.0)]);
    let want = mono(&p, &x);
    let mut d = Id3::new(&p).unwrap();
    let (mut a, mut z) = (x.clone(), vec![0.0; 400]);
    d.process(&mut a[..200], &mut z[..200]);
    for i in 0..PARAMS {
        d.set_param(i, p[i]).unwrap(); // identical re-write
    }
    d.process(&mut a[200..], &mut z[200..]);
    assert_eq!(a, want);
}

#[test]
fn parameters_that_do_not_act_in_the_selected_mode_are_inert() {
    let x = noise(500, 61, 3.0);
    let base = mono(&SONG, &x); // mode 3: P2 (fold) is unused
    for v in [0.0, 0.5, 1.0] {
        assert_eq!(mono(&with(&SONG, &[(2, v)]), &x), base, "P2 in mode 3");
    }
    let m0 = with(&SONG, &[(4, 0.0)]);
    let base = mono(&m0, &x); // mode 0: knee params unused
    for i in 8..11 {
        for v in [0.0, 0.5, 1.0] {
            assert_eq!(mono(&with(&m0, &[(i, v)]), &x), base, "P{i} in mode 0");
        }
    }
}

#[test]
fn zero_ceiling_silences_finite_input_and_is_well_defined() {
    let x = noise(300, 71, 3.0);
    for mode in [0.0f32, 0.3, 0.6, 0.9] {
        let p = with(&FLAT, &[(1, 0.0), (4, mode)]);
        let o = mono(&p, &x);
        // the shaped path is multiplied by the ceiling (0): output is 0 (mix 1), never NaN, for finite input
        assert!(o.iter().all(|v| *v == 0.0), "mode {mode}: {:?}", &o[..4]);
    }
}

#[test]
fn nan_and_infinite_inputs_are_pinned() {
    let p = with(&FLAT, &[(5, 0.5), (3, 0.5)]);
    let o = mono(&p, &[0.1, f32::NAN, 0.2, 0.3]);
    assert!(o[0].is_finite() && o[1].is_nan(), "{o:?}");
    // the next sample's 2x path sees a NaN shaper input, which the final clamp turns into −1 (not NaN): wet = (−1 + 0.2)/2,
    // out = 0.2 + (wet − 0.2)/2 = −0.1; afterwards the history is finite again
    assert!((o[2] + 0.1).abs() < 1e-6 && o[3].is_finite(), "{o:?}");
    // an infinite input leaves `x + (y − x)·mix` = inf − inf = NaN, even for mix 0 (0·inf is NaN as well)
    for mix in [0.0, 0.5, 1.0] {
        let o = mono(&with(&FLAT, &[(3, mix)]), &[0.5, f32::INFINITY, f32::NEG_INFINITY, 0.5]);
        assert!(o[0].is_finite() && o[1].is_nan() && o[2].is_nan() && o[3].is_finite(), "mix {mix}: {o:?}");
    }
}

#[test]
#[should_panic(expected = "equal length required")]
fn mismatched_lengths_panic() {
    Id3::new(&SONG).unwrap().process(&mut [0.0; 4], &mut [0.0; 3]);
}

/// Negative control: a 1/255 nudge of any parameter that acts in the configuration exceeds 1e-4 (baseline < 1e-5), so the
/// tolerance is tight enough to catch a wrong parameter.
#[test]
fn negative_control_nudged_parameters_leave_the_model_tolerance() {
    let x = noise(2000, 81, 3.0);
    let mode0 = with(&FLAT, &[(0, 0.35), (1, 0.8), (2, 0.5), (3, 0.9), (6, 0.6), (7, 0.4), (5, 0.5)]);
    let mode3 = with(&FLAT, &[(0, 0.35), (4, 0.9), (8, 0.4), (9, 0.5), (10, 0.3), (5, 1.0), (6, 0.6), (7, 0.4), (1, 0.8)]);
    for (cfg, acting) in [(mode0, vec![0, 1, 2, 3, 6, 7]), (mode3, vec![0, 1, 8, 9, 10, 6, 7])] {
        let want = model(&cfg, &x);
        assert!(max_err(&mono(&cfg, &x), &want) < 1e-5, "control baseline");
        for i in acting {
            let p = with(&cfg, &[(i, cfg[i] + 1.0 / 255.0)]);
            assert!(max_err(&mono(&p, &x), &want) > 1e-4, "a nudge of P{i} stayed inside the tolerance");
        }
    }
}
