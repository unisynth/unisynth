//! The noise source and the four filter slots, through the whole voice.

use super::*;
use crate::params::old::{self as pm, f as pf, filter_type as ft, filt, n as pn, NOISE};
use synth_core::device::Device;

const RATE: f64 = 44100.0;

fn ftype(i: usize, ty: usize) -> (usize, f32) {
    (filt(i, pf::TYPE), inv::inv_choice(ty, ft::COUNT))
}

fn fcut(i: usize, hz: f32) -> (usize, f32) {
    (filt(i, pf::CUTOFF), inv::inv_cutoff(hz))
}

fn fres(i: usize, p: f32) -> (usize, f32) {
    (filt(i, pf::RESONANCE), p)
}

/// Noise only: oscillator 1 off, noise level `gain` (the gain law, 1 at p = 0.25).
fn noise_only(gain: f32, extra: &[(usize, f32)]) -> Vec<(usize, f32)> {
    let mut e = vec![(osc(0, o::TYPE), inv::inv_choice(osc_type::OFF, osc_type::COUNT)), (pm::v::NOISE_LEVEL, inv::inv_gain(gain))];
    e.extend_from_slice(extra);
    e
}

fn play(edits: &[(usize, f32)], n: usize) -> (Vec<f32>, Vec<f32>) {
    with_ins(&patch(edits), RATE, |ins| render(ins, RATE, &[on(0, 60)], n, 4096))
}

/// Mean power of `x[from..from + n]` between `lo` and `hi` Hz (a DFT on a 25 Hz grid).
fn band(x: &[f32], lo: f64, hi: f64) -> f64 {
    let (from, n) = (4000usize, 16384usize);
    let (mut sum, mut bins) = (0.0f64, 0);
    let mut hz = lo;
    while hz <= hi {
        let (mut re, mut im) = (0.0f64, 0.0f64);
        for (i, v) in x[from..from + n].iter().enumerate() {
            let a = 2.0 * std::f64::consts::PI * hz * (from + i) as f64 / RATE;
            re += *v as f64 * a.cos();
            im += *v as f64 * a.sin();
        }
        sum += (re * re + im * im) / (n * n) as f64;
        bins += 1;
        hz += 25.0;
    }
    sum / bins as f64
}

fn db(a: f64, b: f64) -> f64 {
    10.0 * (a / b).log10()
}

fn rms(x: &[f32]) -> f64 {
    (x.iter().map(|v| (*v as f64).powi(2)).sum::<f64>() / x.len() as f64).sqrt()
}

fn corr(a: &[f32], b: &[f32]) -> f64 {
    let n = a.len() as f64;
    let (ma, mb) = (a.iter().sum::<f32>() as f64 / n, b.iter().sum::<f32>() as f64 / n);
    let (mut ab, mut aa, mut bb) = (0.0, 0.0, 0.0);
    for (x, y) in a.iter().zip(b) {
        let (x, y) = (*x as f64 - ma, *y as f64 - mb);
        ab += x * y;
        aa += x * x;
        bb += y * y;
    }
    ab / (aa * bb).sqrt()
}

/// What a unity noise level becomes: uniform ±1 (rms 1/√3) × centre pan (1/√2) × velocity 100/128 × envelope 1.
fn unit() -> f64 {
    (1.0 / 3.0f64).sqrt() * std::f64::consts::FRAC_1_SQRT_2 * 100.0 / 128.0
}

#[test]
fn the_noise_level_follows_the_gain_law() {
    let one = play(&noise_only(1.0, &[]), 30_000).0;
    assert!((rms(&one[4000..]) / unit() - 1.0).abs() < 0.03, "{} vs {}", rms(&one[4000..]), unit());
    let four = play(&noise_only(4.0, &[]), 30_000).0;
    assert!((rms(&four[4000..]) / rms(&one[4000..]) - 4.0).abs() < 0.1);
    assert_eq!(peak(&play(&noise_only(0.0, &[]), 5000).0), 0.0, "level 0 is silent");
}

#[test]
fn left_and_right_are_decorrelated_and_white() {
    let (l, r) = play(&noise_only(1.0, &[]), 40_000);
    let (l, r) = (&l[4000..], &r[4000..]);
    assert!(corr(l, r).abs() < 0.03, "{}", corr(l, r));
    assert!(corr(&l[..l.len() - 1], &l[1..]).abs() < 0.03, "no memory between samples");
    assert!((l.iter().sum::<f32>() / l.len() as f32).abs() < 0.02, "no DC");
    assert!(l.iter().all(|v| v.abs() <= 1.0));
}

#[test]
fn the_band_filter_shapes_the_noise_around_its_centre() {
    // high-pass at 900 Hz, low-pass at 1100 Hz
    let e = noise_only(
        4.0,
        &[(NOISE + pn::BAND_ON, 1.0), (NOISE + pn::HP, inv::inv_cutoff(900.0)), (NOISE + pn::LP, inv::inv_cutoff(1100.0))],
    );
    let (l, r) = play(&e, 30_000);
    let mid = band(&l, 800.0, 1200.0);
    assert!(db(mid, band(&l, 200.0, 400.0)) > 10.0, "{}", db(mid, band(&l, 200.0, 400.0)));
    assert!(db(mid, band(&l, 3000.0, 5000.0)) > 10.0);
    let peak_hz = (8..=80).map(|k| k as f64 * 25.0).max_by(|a, b| band(&l, *a, *a).partial_cmp(&band(&l, *b, *b)).unwrap()).unwrap();
    assert!((800.0..=1300.0).contains(&peak_hz), "peak at {peak_hz}");
    assert!(corr(&l[4000..], &r[4000..]).abs() < 0.1, "{}", corr(&l[4000..], &r[4000..]));
    // without the band the same noise is flat
    let flat = play(&noise_only(4.0, &[]), 30_000).0;
    assert!(db(band(&flat, 800.0, 1200.0), band(&flat, 3000.0, 5000.0)).abs() < 3.5);
}

#[test]
fn a_hard_right_voice_pan_moves_the_noise_with_it() {
    let (l, r) = play(&noise_only(1.0, &[(pm::v::PAN, 1.0)]), 6000);
    assert_eq!(peak(&l), 0.0);
    assert!(peak(&r) > 0.1);
}

#[cfg(feature = "us-matrix")]
#[test]
fn a_route_can_open_the_noise_level() {
    let e = merged(&[noise_only(0.0, &[]), route(0, source::VELOCITY, target::NOISE_LEVEL, false, 1.0)]);
    assert!(peak(&play(&e, 6000).0) > 0.1);
}

#[test]
fn every_note_has_its_own_noise_and_the_stream_is_deterministic() {
    let run = |call: usize| with_ins(&patch(&noise_only(1.0, &[])), RATE, |ins| render(ins, RATE, &[on(0, 60), off(2000, 60), on(40_000, 60), off(42_000, 60)], 50_000, call));
    let (a, _) = run(50_000);
    assert_ne!(&a[200..1000], &a[40_200..41_000], "a new note draws a new stream");
    for call in [1, 37, 512] {
        assert_eq!(run(call), run(50_000), "call {call}");
    }
    with_ins(&patch(&noise_only(1.0, &[])), RATE, |ins| {
        let x = render(ins, RATE, &[on(0, 60)], 4000, 500);
        ins.reset();
        assert_eq!(render(ins, RATE, &[on(0, 60)], 4000, 500), x);
    });
}

#[test]
fn all_filters_off_is_a_bypass() {
    let plain = play(&noise_only(1.0, &[]), 8000);
    // cutoffs and resonances of an off slot change nothing
    let off = play(&noise_only(1.0, &[fcut(0, 100.0), fres(0, 0.9), fcut(3, 50.0), ftype(1, ft::OFF)]), 8000);
    assert_eq!(plain, off);
    let osc_only = play(&[], 8000);
    assert_eq!(osc_only, play(&[fcut(2, 100.0), fres(2, 0.7)], 8000));
}

#[test]
fn a_low_pass_at_1_khz_keeps_the_octave_below_and_cuts_the_octave_above() {
    for ty in [ft::SVF_LP, ft::LADDER_LP] {
        // the cutoff parameter at 1 kHz (the SVF's own corner is then at 750 Hz, WaveSabre's coefficient)
        let (l, _) = play(&noise_only(4.0, &[ftype(0, ty), fcut(0, 1000.0)]), 30_000);
        let (below, above) = (band(&l, 450.0, 550.0), band(&l, 1900.0, 2100.0));
        assert!(db(below, above) >= 9.0, "type {ty}: {}", db(below, above));
    }
}

#[test]
fn each_slot_filters_and_slots_run_in_series() {
    let one = |i: usize| {
        let (l, _) = play(&noise_only(4.0, &[ftype(i, ft::SVF_LP), fcut(i, 1333.0)]), 30_000);
        db(band(&l, 450.0, 550.0), band(&l, 3800.0, 4200.0))
    };
    let single = one(0);
    for i in 1..4 {
        assert!((one(i) - single).abs() < 0.5, "slot {i}: {} vs {single}", one(i));
    }
    // two in series: twice the attenuation in dB
    let (l, _) = play(&noise_only(4.0, &[ftype(0, ft::SVF_LP), fcut(0, 1333.0), ftype(2, ft::SVF_LP), fcut(2, 1333.0)]), 30_000);
    let both = db(band(&l, 450.0, 550.0), band(&l, 3800.0, 4200.0));
    assert!((both / single - 2.0).abs() < 0.15, "{both} vs 2 × {single}");
    // a high-pass after a low-pass leaves a band
    let (l, _) = play(&noise_only(4.0, &[ftype(0, ft::SVF_LP), fcut(0, 4000.0), ftype(1, ft::RBJ_HP), fcut(1, 1000.0)]), 30_000);
    assert!(db(band(&l, 1800.0, 2400.0), band(&l, 100.0, 200.0)) > 12.0);
}

#[test]
fn every_type_changes_the_sound_in_its_own_way() {
    let spectrum = |ty: usize| {
        let (l, _) = play(&noise_only(4.0, &[ftype(0, ty), fcut(0, 1500.0), fres(0, 0.3)]), 30_000);
        [band(&l, 100.0, 300.0), band(&l, 1300.0, 1700.0), band(&l, 6000.0, 9000.0)]
    };
    let flat = spectrum(ft::OFF);
    let tilt = |s: [f64; 3]| (db(s[0], flat[0]), db(s[1], flat[1]), db(s[2], flat[2]));
    let lows = |ty| tilt(spectrum(ty));
    for ty in [ft::SVF_LP, ft::LADDER_LP] {
        let (lo, _, hi) = lows(ty);
        assert!(hi < lo - 10.0, "low-pass {ty}: {lo} {hi}");
    }
    for ty in [ft::SVF_HP, ft::RBJ_HP] {
        let (lo, _, hi) = lows(ty);
        assert!(lo < hi - 10.0, "high-pass {ty}: {lo} {hi}");
    }
    for ty in [ft::SVF_BP, ft::RBJ_BP] {
        let (lo, mid, hi) = lows(ty);
        assert!(mid > lo + 3.0 && mid > hi + 3.0, "band-pass {ty}: {lo} {mid} {hi}");
    }
    // notches: a dip at their own corner (the SVF's is 3/4 of its cutoff parameter) against the bands beside it
    for (ty, lo, hi) in [(ft::SVF_NOTCH, 1050.0, 1200.0), (ft::NOTCH, 1450.0, 1550.0)] {
        let (l, _) = play(&noise_only(4.0, &[ftype(0, ty), fcut(0, 1500.0), fres(0, 0.8)]), 30_000);
        let dip = band(&l, lo, hi);
        assert!(db(band(&l, 300.0, 500.0), dip) > 6.0, "notch {ty}: {}", db(band(&l, 300.0, 500.0), dip));
    }
}

#[test]
fn resonance_is_each_types_original_law_through_the_voice() {
    // the SVF scales its input by q = 1 − p: a sine far below the corner (note 60, corner 6 kHz) comes out at q times its level
    let level = |p: f32| {
        let e = [fres(0, p), ftype(0, ft::SVF_LP), fcut(0, 8000.0)];
        peak(&play(&e, 6000).0[1000..])
    };
    assert!((level(0.5) / level(0.0) - 0.5).abs() < 0.03, "{} {}", level(0.5), level(0.0));
    // the others clamp the parameter at 0.85
    let clamped = |p: f32| play(&noise_only(4.0, &[ftype(0, ft::LADDER_LP), fcut(0, 3000.0), fres(0, p)]), 12_000).0;
    assert_eq!(clamped(0.85), clamped(1.0));
    assert_ne!(clamped(0.2), clamped(0.6));
}

#[cfg(feature = "us-matrix")]
#[test]
fn the_cutoff_routes_scale_and_offset() {
    let base = noise_only(4.0, &[ftype(0, ft::SVF_LP), fcut(0, 700.0)]);
    let hf = |edits: &[(usize, f32)]| {
        let (l, _) = play(edits, 30_000);
        band(&l, 3000.0, 5000.0)
    };
    let plain = hf(&base);
    // Offset: velocity 0.78 × 10 000 Hz
    let up = merged(&[base.clone(), route(0, source::VELOCITY, target::filter_cutoff(0), false, 10_000.0)]);
    assert!(db(hf(&up), plain) > 15.0, "{}", db(hf(&up), plain));
    // Scale: the normalized cutoff (0.186 for 700 Hz) × (amount × velocity)
    let scaled = merged(&[base.clone(), route(0, source::VELOCITY, target::filter_cutoff(0), true, 4.0)]);
    assert!(db(hf(&scaled), plain) > 10.0, "{}", db(hf(&scaled), plain));
    // and a route on the resonance reaches the filter
    let q = merged(&[base.clone(), route(0, source::VELOCITY, target::filter_resonance(0), false, 1.0)]);
    assert!((rms(&play(&q, 20_000).0[4000..]) / rms(&play(&base, 20_000).0[4000..]) - 1.0).abs() > 0.1);
}

#[test]
fn a_type_can_be_automated_while_a_note_sounds() {
    let e = noise_only(4.0, &[ftype(0, ft::SVF_LP), fcut(0, 900.0)]);
    with_ins(&patch(&e), RATE, |ins| {
        let (a, _) = render(ins, RATE, &[on(0, 60)], 24_000, 512);
        // high over low, as the low-pass leaves it, and after the switch
        let tilt = |x: &[f32]| db(band(x, 3000.0, 5000.0), band(x, 300.0, 600.0));
        let before = tilt(&a);
        assert!(before < -6.0, "{before}");
        // the low-pass becomes a high-pass between two calls
        ins.set_param(v3::filt(0, v3::f::TYPE), v3::filter_type::encode(v3::filter_type::SVF_HP));
        let mut rng = Lcg(1);
        let (mut l, mut r, mut c, mut d) = (vec![0.0; 24_000], vec![0.0; 24_000], vec![0.0; 24_000], vec![0.0; 24_000]);
        let mut ctx = Ctx { rate: RATE, tempo: 120, frame: 24_000, rng: &mut rng };
        ins.run(&mut Io { ch: [&mut l, &mut r, &mut c, &mut d] }, 24_000, &mut ctx);
        assert!(l.iter().all(|v| v.is_finite()));
        assert!(tilt(&l) > before + 8.0, "{} then {}", before, tilt(&l));
    });
}


#[cfg(feature = "us-flt-svf")]
#[test]
fn a_low_pass_at_the_top_of_its_range_is_passed_through() {
    // 440 Hz: the open state-variable low-pass is 0.02 dB down there, so the voice with the filter and the voice without it
    // agree to a fraction of a percent; the one with it sounds exactly like the one with the slot off
    let off = play(&[ftype(0, ft::OFF)], 20_000).0;
    let open = play(&[ftype(0, ft::SVF_LP), fcut(0, 20_000.0), fres(0, 0.0)], 20_000).0;
    assert!(peak(&off) > 0.01);
    let diff: Vec<f32> = off.iter().zip(&open).map(|(a, b)| a - b).collect();
    assert!(rms(&diff[4000..]) < 0.01 * rms(&off[4000..]), "{} against {}", rms(&diff[4000..]), rms(&off[4000..]));
    // a cutoff one step below the range runs the filter: the output is no longer the input
    let shut = play(&[ftype(0, ft::SVF_LP), fcut(0, 2_000.0), fres(0, 0.0)], 20_000).0;
    let d2: Vec<f32> = off.iter().zip(&shut).map(|(a, b)| a - b).collect();
    assert!(rms(&d2[4000..]) > rms(&diff[4000..]), "{} vs {}", rms(&d2[4000..]), rms(&diff[4000..]));
}

#[cfg(feature = "us-matrix-add")]
#[test]
fn an_add_route_moves_the_normalized_cutoff_before_its_law() {
    use crate::modmx::op;
    let base = noise_only(4.0, &[ftype(0, ft::SVF_LP), fcut(0, 700.0)]);
    let hf = |edits: &[(usize, f32)]| {
        let (l, _) = play(edits, 30_000);
        band(&l, 3000.0, 5000.0)
    };
    let plain = hf(&base);
    let cut = pm::filt(0, pm::f::CUTOFF);
    let t = target::PARAM + target::PARAMS.iter().position(|&i| i == v3i(cut)).unwrap() as u8;
    let add = |amount: f32| {
        let mut e = base.clone();
        e.extend([
            (pm::modslot(13, pm::m::SOURCE), source_old(source::VELOCITY)),
            (pm::modslot(13, pm::m::TARGET), crate::law::host::inv_index(t)),
            (pm::modslot(13, pm::m::OPERATION), op_old(op::ADD)),
            (pm::modslot(13, pm::m::AMOUNT), (amount + 1.0) / 2.0),
        ]);
        e
    };
    assert!(db(hf(&add(1.0)), plain) > 15.0, "velocity opens the filter: {}", db(hf(&add(1.0)), plain));
    assert!(db(hf(&add(-1.0)), plain) < -6.0, "and closes it: {}", db(hf(&add(-1.0)), plain));
}
