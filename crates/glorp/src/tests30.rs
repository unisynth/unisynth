//! Synthetic only, ID 30 (no song or tap data): the API contract (rejection table, atomicity, the cascade-topology
//! coupling and its order hazard, pool size), the bypass, the gain laws, the hold, an independent f64 model of the
//! plain delay-tap path, the reverb's echo times against the published tunings, the grain engine's structural laws,
//! block-split bit identity, `reset`, and negative controls. The reference parity itself lives in
//! `tests/reference30.rs` (env-gated).

use crate::id30::{check, Error, Id30, PARAMS, SLAB_F32};
use crate::id34::{tick, Hist, CHAIN_A};
use synth_core::pool::Pool;

/// A supported, musical parameter set (made up here): two grain streams, light reverb, no cascade.
const BASE: [f32; PARAMS] =
    [0.1, 0.5, 0.5, 0.7, 0.5, 0.8, 0.05, 0.5, 0.4, 0.2, 0.2, 0.0, 0.6, 0.4, 0.5, 0.0, 0.0, 0.3, 0.7, 0.2, 0.0, 0.3, 0.4, 1.0];

fn p(edits: &[(usize, f32)]) -> [f32; PARAMS] {
    let mut v = BASE;
    for &(i, x) in edits {
        v[i] = x;
    }
    v
}

fn arena() -> Vec<u8> {
    vec![0u8; SLAB_F32 * 4 + 64]
}

/// Runs `f` on a fresh device (the lines live in a test-owned arena).
fn with<R>(params: &[f32; PARAMS], f: impl FnOnce(&mut Id30) -> R) -> R {
    let mut mem = arena();
    let mut pool = Pool::new(&mut mem);
    let mut d = Id30::new(params, &mut pool).expect("supported parameters");
    f(&mut d)
}

struct Lcg(u64);
impl Lcg {
    fn next(&mut self) -> f32 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        ((self.0 >> 40) as f32 / (1u64 << 24) as f32) * 2.0 - 1.0
    }
}

fn noise(n: usize, amp: f32, seed: u64) -> Vec<f32> {
    let mut g = Lcg(seed);
    (0..n).map(|_| g.next() * amp).collect()
}

/// Processes `n` samples in calls of `chunk` frames; returns (L, R).
fn run(d: &mut Id30, l: &[f32], r: &[f32], chunk: usize) -> (Vec<f32>, Vec<f32>) {
    let (mut ol, mut or) = (l.to_vec(), r.to_vec());
    for (a, b) in ol.chunks_mut(chunk).zip(or.chunks_mut(chunk)) {
        d.process(a, b);
    }
    (ol, or)
}

#[test]
fn the_rejection_table_is_exact_and_atomic() {
    with(&BASE, |d| {
        let before = run(d, &noise(256, 0.3, 1), &noise(256, 0.3, 2), 128);
        for (i, v, want) in [
            (24usize, 0.5f32, Error::Index),
            (255, 0.5, Error::Index),
            (0, f32::NAN, Error::Range),
            (0, f32::INFINITY, Error::Range),
            (0, f32::NEG_INFINITY, Error::Range),
            (1, -1e-30, Error::Range),
            (1, 1.000_000_1, Error::Range),
            (23, -0.1, Error::Range),
        ] {
            assert_eq!(d.set_param(i, v), Err(want), "P{i} = {v}");
        }
        for ok in [0.0f32, 1.0, 0.5, 0.000_001] {
            assert_eq!(d.set_param(0, ok), Ok(()));
        }
        // atomic: a refused write left the device as it was (twin built from the same history agrees to the bit)
        let mut twin_mem = arena();
        let mut tp = Pool::new(&mut twin_mem);
        let mut twin = Id30::new(&BASE, &mut tp).unwrap();
        run(&mut twin, &noise(256, 0.3, 1), &noise(256, 0.3, 2), 128);
        d.set_param(0, BASE[0]).unwrap();
        for bad in [(24usize, 0.5f32), (0, f32::NAN), (23, -0.1)] {
            let _ = d.set_param(bad.0, bad.1);
        }
        let (a, b) = (noise(128, 0.3, 3), noise(128, 0.3, 4));
        assert_eq!(run(d, &a, &b, 128), run(&mut twin, &a, &b, 128));
        let _ = before;
    });
    let mut v = BASE;
    v[3] = 1.5;
    assert_eq!(check(&v), Err(Error::Range));
    let mut mem = arena();
    assert!(matches!(Id30::new(&v, &mut Pool::new(&mut mem)), Err(Error::Range)));
}

#[test]
fn the_cascade_needs_the_delay_line_topology_and_the_check_is_cross_parameter() {
    // trunc(2.999·P20): 0 for P20 ≤ 0.33344, so 0.3334 is the delay-line topology and 0.3335 the first-order one
    for (p20, ok) in [(0.0f32, true), (0.3334, true), (0.3335, false), (0.7, false), (1.0, false)] {
        assert_eq!(check(&p(&[(16, 0.5), (20, p20)])).is_ok(), ok, "cascade on, P20 = {p20}");
        // cascade off (P16 ≤ 0.001): every topology is accepted, the cascade is never run
        assert_eq!(check(&p(&[(16, 0.0), (20, p20)])), Ok(()), "cascade off, P20 = {p20}");
        assert_eq!(check(&p(&[(16, 0.001), (20, p20)])), Ok(()), "P16 = 0.001 is still off, P20 = {p20}");
    }
    assert_eq!(check(&p(&[(16, 0.001_000_1), (20, 0.5)])), Err(Error::Topology));
    assert_eq!(check(&p(&[(16, 0.001_000_1), (20, 0.0)])), Ok(()));
    // order hazard: a write is judged on the whole resulting vector, so move the topology first / the mix last
    with(&p(&[(16, 0.0), (20, 0.7)]), |d| {
        assert_eq!(d.set_param(16, 0.5), Err(Error::Topology), "cascade on over topology 2");
        assert_eq!(d.set_param(20, 0.0), Ok(()));
        assert_eq!(d.set_param(16, 0.5), Ok(()));
        assert_eq!(d.set_param(20, 0.7), Err(Error::Topology), "topology 2 under a live cascade");
        assert_eq!(d.set_param(16, 0.0), Ok(()));
        assert_eq!(d.set_param(20, 0.7), Ok(()));
    });
}

#[test]
fn the_pool_must_hold_the_lines() {
    let mut small = vec![0u8; SLAB_F32 * 4 - 1];
    assert!(matches!(Id30::new(&BASE, &mut Pool::new(&mut small)), Err(Error::Memory)));
    // the line memory is zeroed: a device on dirty memory behaves as on clean memory
    let mut dirty = vec![0xA5u8; SLAB_F32 * 4 + 64];
    let (l, r) = (noise(512, 0.3, 5), noise(512, 0.3, 6));
    let a = run(&mut Id30::new(&BASE, &mut Pool::new(&mut dirty)).unwrap(), &l, &r, 128);
    let b = with(&BASE, |d| run(d, &l, &r, 128));
    assert_eq!(a, b);
    assert!(SLAB_F32 * 4 < 5 << 20, "about 4 MB per device");
}

#[test]
fn a_disabled_device_leaves_the_block_alone_and_freezes_its_state() {
    let (l, r) = (noise(640, 0.3, 7), noise(640, 0.3, 8));
    let fresh = with(&BASE, |d| run(d, &l, &r, 128));
    with(&BASE, |d| {
        d.set_param(23, 0.4999).unwrap();
        let (ol, or) = run(d, &l, &r, 128);
        assert_eq!((ol, or), (l.clone(), r.clone()), "bypass is the identity");
        // nothing moved while disabled: the next enabled run is the first run of a fresh device
        d.set_param(23, 0.5).unwrap();
        assert_eq!(run(d, &l, &r, 128), fresh);
    });
    // enabled exactly at 0.5
    with(&p(&[(23, 0.5)]), |d| assert_ne!(run(d, &l, &r, 128).0, l));
}

#[test]
fn dry_and_wet_gains_are_exact_laws() {
    let (l, r) = (noise(512, 0.3, 9), noise(512, 0.3, 10));
    // wet gain 0: out = in · P2 bit for bit
    with(&p(&[(3, 0.0), (2, 0.5)]), |d| {
        let (ol, or) = run(d, &l, &r, 128);
        for i in 0..512 {
            assert_eq!((ol[i], or[i]), (l[i] * 0.5, r[i] * 0.5), "sample {i}");
        }
    });
    // both gains 0: silence
    with(&p(&[(3, 0.0), (2, 0.0)]), |d| {
        let (ol, or) = run(d, &l, &r, 128);
        assert!(ol.iter().chain(&or).all(|&x| x == 0.0));
    });
    // the wet part adds linearly: out(P2, P3) = P3·wet + P2·in, wet from the P2 = 0, P3 = 1 run
    let wet = with(&p(&[(2, 0.0), (3, 1.0)]), |d| run(d, &l, &r, 128));
    let mix = with(&p(&[(2, 0.25), (3, 0.5)]), |d| run(d, &l, &r, 128));
    for i in 0..512 {
        let want = wet.0[i] * 0.5 + l[i] * 0.25;
        assert!((mix.0[i] - want).abs() <= 1e-6, "left sample {i}: {} vs {want}", mix.0[i]);
    }
}

#[test]
fn hold_from_the_start_keeps_the_line_empty() {
    // P11 > 0.5 never writes the delay line: with nothing in it there is no wet signal, whatever the feedback
    let (l, r) = (noise(1024, 0.4, 11), noise(1024, 0.4, 12));
    for streams in [0.0f32, 0.4, 1.0] {
        with(&p(&[(11, 0.5001), (1, 0.9), (2, 0.5), (3, 1.0), (8, streams)]), |d| {
            let (ol, or) = run(d, &l, &r, 128);
            for i in 0..1024 {
                assert_eq!((ol[i], or[i]), (l[i] * 0.5, r[i] * 0.5), "streams {streams} sample {i}");
            }
        });
    }
    // exactly 0.5 is not a hold
    with(&p(&[(11, 0.5), (8, 0.0), (0, 0.001), (3, 1.0), (2, 0.0)]), |d| {
        let (ol, _) = run(d, &l, &r, 1024);
        assert!(ol[600..].iter().any(|&x| x.abs() > 1e-3));
    });
}

// ---- an independent f64 model of the plain delay-tap path (P8 = 0, no reverb, no feedback) ----

fn rbj(high: bool, fc: f64, q: f64) -> [f64; 5] {
    let w = 2.0 * std::f64::consts::PI * fc / 44100.0;
    let (sn, cs) = (w.sin(), w.cos());
    let alpha = sn / (2.0 * q);
    let (b0, b1) = if high { ((1.0 + cs) / 2.0, -(1.0 + cs)) } else { ((1.0 - cs) / 2.0, 1.0 - cs) };
    let a0 = 1.0 + alpha;
    [b0 / a0, b1 / a0, b0 / a0, -2.0 * cs / a0, (1.0 - alpha) / a0]
}

fn biquad(c: &[f64; 5], x: &[f64]) -> Vec<f64> {
    let (mut x1, mut x2, mut y1, mut y2) = (0.0, 0.0, 0.0, 0.0);
    x.iter()
        .map(|&v| {
            let y = c[0] * v + c[1] * x1 + c[2] * x2 - c[3] * y1 - c[4] * y2;
            (x2, x1, y2, y1) = (x1, v, y1, y);
            y
        })
        .collect()
}

/// Model of `out = P3·HP(LP(delay(in))) + P2·in` with the delay gliding from 0 to P0 seconds (`+= (T − t)·0.021`
/// per written sample) and read linearly `t·44100` samples behind the newest write (ideal f64 positions).
fn tap_model(params: &[f32; PARAMS], x: &[f64], written: &[f64]) -> Vec<f64> {
    let (target, mut time) = (params[0] as f64, 0.0f64);
    let mut line = vec![0.0f64; x.len() + 2];
    let mut wet = Vec::with_capacity(x.len());
    let mut newest = 0usize; // index of the newest written sample + 1 (line[0] is the zero before the first write)
    for &v in written {
        let pos = newest as f64 - time * 44100.0;
        let i = pos.floor();
        let f = pos - i;
        let at = |k: f64| if k < 0.0 { 0.0 } else { line[k as usize] };
        wet.push(at(i) * (1.0 - f) + at(i + 1.0) * f);
        time += (target - time) * 0.021;
        newest += 1;
        line[newest] = v;
    }
    let lp = rbj(false, (params[5] as f64).powi(2) * 20000.0 + 20.0, 0.7);
    let hp = rbj(true, (params[6] as f64).powi(2) * 20000.0 + 20.0, 0.7);
    let f = biquad(&hp, &biquad(&lp, &wet));
    x.iter().zip(&f).map(|(&d, &w)| w * params[3] as f64 + d * params[2] as f64).collect()
}

/// What the feedback path writes into the line with no shift and no feedback: the input through the first all-pass
/// chain of the frequency shifter (the 90° branch has the weight `sin 2π = 0`); channel mix P22 = 0 keeps L and R apart.
fn written(x: &[f32]) -> Vec<f64> {
    let mut st = [Hist::default(); 4];
    x.iter()
        .map(|&v| {
            let mut a = v;
            for k in 0..4 {
                a = tick(&CHAIN_A[k], &mut st[k], a);
            }
            a as f64
        })
        .collect()
}

fn tap_params(extra: &[(usize, f32)]) -> [f32; PARAMS] {
    let mut v = p(&[(8, 0.0), (14, 0.0), (1, 0.0), (0, 0.004), (5, 0.7), (6, 0.1), (2, 0.3), (3, 0.9), (22, 0.0)]);
    for &(i, x) in extra {
        v[i] = x;
    }
    v
}

fn sine(n: usize, hz: f64, amp: f64) -> Vec<f32> {
    (0..n).map(|i| (amp * (2.0 * std::f64::consts::PI * hz * i as f64 / 44100.0).sin()) as f32).collect()
}

fn max_err(got: &[f32], want: &[f64]) -> f64 {
    got.iter().zip(want).map(|(&g, &w)| (g as f64 - w).abs()).fold(0.0, f64::max)
}

#[test]
fn the_plain_tap_matches_an_independent_model() {
    // a smooth input keeps the f32 read-position rounding (≤ 0.001 sample at these indices) far below the bound
    let x = sine(80 * 128, 300.0, 0.5);
    for params in [tap_params(&[]), tap_params(&[(0, 0.0)]), tap_params(&[(0, 0.02), (5, 0.3), (6, 0.0)]), tap_params(&[(2, 0.0), (3, 1.0), (6, 0.4)])] {
        let (ol, _) = with(&params, |d| run(d, &x, &x, 128));
        let xs: Vec<f64> = x.iter().map(|&v| v as f64).collect();
        let want = tap_model(&params, &xs, &written(&x));
        let e = max_err(&ol, &want);
        assert!(e < 5e-4, "P0 {} P5 {} P6 {}: max error {e:e}", params[0], params[5], params[6]);
    }
}

#[test]
fn the_tap_model_notices_wrong_parameters() {
    // negative control: the model with a nudged parameter leaves the bound the faithful one meets
    let x = sine(80 * 128, 300.0, 0.5);
    let xs: Vec<f64> = x.iter().map(|&v| v as f64).collect();
    let params = tap_params(&[]);
    let (ol, _) = with(&params, |d| run(d, &x, &x, 128));
    for (i, delta) in [(0usize, 0.002f32), (2, 0.05), (3, 0.05), (5, 0.1), (6, 0.2)] {
        let mut m = params;
        m[i] += delta;
        let e = max_err(&ol, &tap_model(&m, &xs, &written(&x)));
        assert!(e > 1e-3, "nudging P{i} by {delta} left error {e:e}");
    }
}

// ---- the reverb against its published tunings ----

#[test]
fn the_reverb_starts_after_the_shortest_comb_and_the_right_side_is_spread_by_23() {
    // before the first comb read-back the reverb output is exactly zero; the onset relative to the plain tap (both
    // through the same shifter chain, whose IIR response starts on the impulse) is the comb length (+ the one-call
    // write/read order), and the right side's combs are 23 samples longer
    let go = |mix: f32| {
        let params = p(&[(8, 0.0), (0, 0.0), (1, 0.0), (12, 0.5), (14, mix), (5, 1.0), (6, 0.0), (2, 0.0), (3, 1.0), (22, 0.0)]);
        let mut x = vec![0.0f32; 80 * 128];
        x[500] = 1.0;
        with(&params, |d| run(d, &x, &x, 128))
    };
    let onset = |v: &[f32]| v.iter().position(|&s| s != 0.0).unwrap();
    let (tap, rev) = (go(0.0), go(1.0));
    let (tl, tr) = (onset(&tap.0), onset(&tap.1));
    assert_eq!(tl, tr);
    let (rl, rr) = (onset(&rev.0), onset(&rev.1));
    assert!((1116..=1118).contains(&(rl - tl)), "left onset {}", rl - tl);
    assert_eq!(rr - rl, 23, "right comb spread");
}

#[test]
fn reverb_feedback_decays_and_freeze_sustains() {
    let mut imp = vec![0.0f32; 300 * 128];
    imp[3000] = 0.5;
    let late = |v: &[f32]| v[20000..].iter().map(|&x| (x as f64).powi(2)).sum::<f64>();
    let tail = |edits: &[(usize, f32)]| {
        let params = p(&[&[(8usize, 0.0f32), (0, 0.0), (1, 0.0), (14, 1.0), (2, 0.0), (3, 1.0), (13, 0.0)][..], edits].concat());
        with(&params, |d| run(d, &imp, &imp, 128).0)
    };
    let low = late(&tail(&[(12, 0.1)]));
    let high = late(&tail(&[(12, 0.9)]));
    let frozen = late(&tail(&[(12, 0.1), (15, 1.0)]));
    assert!(high > 100.0 * low, "feedback 0.9 rings far longer than 0.1: {high:e} vs {low:e}");
    assert!(frozen > high, "freeze rings longest: {frozen:e} vs {high:e}");
}

// ---- the grain engine ----

#[test]
fn grain_streams_with_no_scatter_are_copies_of_each_other() {
    // every stream starts at the read position and the hash only moves it through the scatter: with P9 = 0 the streams
    // are identical, so the 1/N normalisation makes the output independent of the stream count
    let (l, r) = (noise(60 * 128, 0.5, 13), noise(60 * 128, 0.5, 14));
    let one = with(&p(&[(8, 0.2), (9, 0.0), (14, 0.0), (1, 0.0), (6, 0.5)]), |d| run(d, &l, &r, 128));
    for streams in [0.4f32, 0.6, 0.8, 1.0] {
        let many = with(&p(&[(8, streams), (9, 0.0), (14, 0.0), (1, 0.0), (6, 0.5)]), |d| run(d, &l, &r, 128));
        let e = max_err(&many.0, &one.0.iter().map(|&x| x as f64).collect::<Vec<_>>());
        assert!(e < 1e-5, "{streams}: {e:e}");
    }
    // with scatter they differ
    let a = with(&p(&[(8, 0.2), (9, 0.5), (14, 0.0), (1, 0.0)]), |d| run(d, &l, &r, 128));
    let b = with(&p(&[(8, 1.0), (9, 0.5), (14, 0.0), (1, 0.0)]), |d| run(d, &l, &r, 128));
    assert_ne!(a.0, b.0);
}

#[test]
fn the_stream_count_switches_between_the_tap_and_the_grains_at_one_fifth() {
    let (l, r) = (noise(40 * 128, 0.5, 15), noise(40 * 128, 0.5, 16));
    let go = |v: f32| with(&p(&[(8, v), (14, 0.0), (0, 0.03)]), |d| run(d, &l, &r, 128));
    // trunc(5·0.19999) = 0 (tap), trunc(5·0.2) = 1: the tap and the first grain stream differ; 0.19999 equals 0.0
    assert_eq!(go(0.19999), go(0.0));
    assert_ne!(go(0.2), go(0.0));
    assert_eq!(go(0.2), go(0.399_99), "one stream up to 0.4");
    assert_ne!(go(0.4), go(0.2));
}

#[test]
fn a_zero_length_grain_is_silent_not_undefined() {
    // P10 = 0: the window argument is 0/0; the reference's table sine answers ~ 0, never NaN, and nothing is read out of range
    let (l, r) = (noise(40 * 128, 0.5, 17), noise(40 * 128, 0.5, 18));
    let (ol, or) = with(&p(&[(10, 0.0), (14, 0.0), (8, 1.0), (2, 0.0), (3, 1.0)]), |d| run(d, &l, &r, 128));
    assert!(ol.iter().chain(&or).all(|x| x.is_finite() && x.abs() < 1e-3));
}

// ---- structure ----

#[test]
fn processing_is_independent_of_how_the_block_is_split() {
    let (l, r) = (noise(1024, 0.5, 19), noise(1024, 0.5, 20));
    for params in [BASE, p(&[(16, 0.7), (4, 0.2), (14, 0.3)]), p(&[(11, 0.9)]), p(&[(8, 0.0), (21, 0.0), (22, 1.0)])] {
        let whole = with(&params, |d| run(d, &l, &r, 1024));
        for chunk in [128usize, 64, 1, 100, 333] {
            let split = with(&params, |d| run(d, &l, &r, chunk));
            assert_eq!(whole, split, "chunk {chunk}");
        }
    }
}

#[test]
fn reset_is_a_fresh_device_and_parameters_stay() {
    let (l, r) = (noise(512, 0.5, 21), noise(512, 0.5, 22));
    let params = p(&[(16, 0.6), (4, 0.3)]);
    let fresh = with(&params, |d| run(d, &l, &r, 128));
    with(&params, |d| {
        run(d, &noise(2048, 0.8, 23), &noise(2048, 0.8, 24), 128);
        d.reset();
        assert_eq!(run(d, &l, &r, 128), fresh);
    });
}

#[test]
fn the_cascade_is_skipped_below_the_threshold_and_audible_above() {
    let (l, r) = (noise(40 * 128, 0.5, 25), noise(40 * 128, 0.5, 26));
    let off = with(&p(&[(16, 0.001), (1, 0.7), (2, 0.0)]), |d| run(d, &l, &r, 128));
    let none = with(&p(&[(16, 0.0), (1, 0.7), (2, 0.0)]), |d| run(d, &l, &r, 128));
    assert_eq!(off, none, "P16 = 0.001 is off");
    let on = with(&p(&[(16, 0.002), (1, 0.7), (2, 0.0)]), |d| run(d, &l, &r, 128));
    assert_ne!(on.0, none.0, "P16 = 0.002 runs the cascade");
    // the cascade is all-pass: at full mix the wet energy is the shifted signal's (finite, same order)
    let full = with(&p(&[(16, 1.0), (1, 0.0), (2, 0.0), (14, 0.0)]), |d| run(d, &l, &r, 128));
    assert!(full.0.iter().all(|x| x.is_finite()));
}

#[test]
fn random_supported_vectors_without_outer_feedback_stay_finite() {
    let mut g = Lcg(99);
    let (l, r) = (noise(50 * 128, 0.5, 27), noise(50 * 128, 0.5, 28));
    let mut ran = 0;
    for _ in 0..40 {
        let mut v = [0.0f32; PARAMS];
        for x in v.iter_mut() {
            *x = (g.next() + 1.0) / 2.0;
        }
        v[1] = 0.0; // feedback of the wet signal into the line is what can run away; everything else is bounded
        v[23] = 1.0;
        if check(&v).is_err() {
            v[20] = 0.0;
        }
        let (ol, or) = with(&v, |d| run(d, &l, &r, 128));
        assert!(ol.iter().chain(&or).all(|x| x.is_finite()), "{v:?}");
        ran += 1;
    }
    assert_eq!(ran, 40);
}

/// Characterisation, not a reference vector: the device does not sanitise its input. One NaN input sample enters the
/// delay line / filters / reverb and keeps the output NaN on later clean blocks until `reset`. Whether the original does
/// the same is NOT reference-verified (no recorded non-finite input).
#[test]
fn a_nan_input_sample_poisons_the_state_until_reset() {
    let params = p(&[(16, 0.6), (4, 0.3)]);
    let (cl, cr) = (noise(128, 0.5, 31), noise(128, 0.5, 32));
    with(&params, |d| {
        let (mut l, mut r) = (cl.clone(), cr.clone());
        l[7] = f32::NAN;
        d.process(&mut l, &mut r);
        for _ in 0..8 {
            let (mut l, mut r) = (cl.clone(), cr.clone());
            d.process(&mut l, &mut r);
            assert!(l.iter().chain(&r).any(|x| x.is_nan()), "state not poisoned");
        }
        d.reset();
        let (mut l, mut r) = (cl.clone(), cr.clone());
        d.process(&mut l, &mut r);
        assert!(l.iter().chain(&r).all(|x| x.is_finite()));
    });
}
