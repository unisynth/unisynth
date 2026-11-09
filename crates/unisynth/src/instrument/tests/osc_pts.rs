//! The point oscillator (change add-tf3-import, Tunefish 3's oscillator): the drawn cycle, its modulation, layers.

use super::{lfo_1hz_v3, on, render, route_v3, with_ins, UInstrument};
use crate::law::host as inv;
use crate::params::{o, osc, osc_type, pt, pts, shape as sp, DEFAULTS};

fn sh(k: usize) -> usize {
    osc(0, o::SHAPE + k)
}

/// A v3 patch (the shared helpers work in the old layout, which has no point type).
fn patch(edits: &[(usize, f32)]) -> Vec<u8> {
    let mut v = DEFAULTS;
    v[osc(0, o::TYPE)] = osc_type::encode(osc_type::PM);
    for &(i, x) in edits {
        v[i] = x;
    }
    crate::chunk::encode(&v, &[])
}

/// Slot 1 = point oscillator with `n` points `(offset, value)` and every segment in `mode`, one layer, drive 0.
fn points(n: usize, at: &[(f32, f32)], mode: usize, extra: &[(usize, f32)]) -> Vec<(usize, f32)> {
    let mut e = vec![
        (osc(0, o::TYPE), inv::inv_choice(osc_type::POINTS, osc_type::COUNT)),
        (pts(0, pt::COUNT_), inv::inv_count(n as u32, 1, 4)),
    ];
    for (k, &(off, val)) in at.iter().enumerate() {
        e.push((sh(sp::PTS_POINT + 2 * k), off));
        e.push((sh(sp::PTS_POINT + 2 * k + 1), val));
    }
    for k in 0..pt::MODES {
        e.push((pts(0, pt::MODE + k), inv::inv_choice(mode, 3)));
    }
    e.extend_from_slice(extra);
    e
}

fn play(edits: &[(usize, f32)], note: u8, n: usize) -> (Vec<f32>, Vec<f32>) {
    with_ins(&patch(edits), 44100.0, |ins| render(ins, 44100.0, &[on(0, note)], n, 512))
}

#[test]
fn a_step_cycle_is_a_square_between_two_levels() {
    // one point half-way at full value, step segments: half the cycle at the low level, half at the high one (the
    // decimation stage's 12.83 kHz low-pass rings briefly at each edge)
    let (x, _) = play(&points(1, &[(0.5, 1.0)], pt::STEP, &[]), 57, 22050);
    let tail = &x[4410..];
    let mut sorted = tail.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let (lo, hi) = (sorted[sorted.len() / 4], sorted[sorted.len() * 3 / 4]); // the plateaus, not the ringing
    assert!(hi > 0.1 && (hi + lo).abs() < 0.02 * hi, "symmetric levels: {hi} {lo}");
    let at = |lv: f32| tail.iter().filter(|v| (**v - lv).abs() < 0.02 * hi).count() as f32 / tail.len() as f32;
    assert!(at(hi) > 0.4 && at(lo) > 0.4, "two levels, about half each: {} {}", at(hi), at(lo));
    // one period per note cycle: note 57 = 220 Hz
    let zc = tail.windows(2).filter(|w| w[0] <= 0.0 && w[1] > 0.0).count() as f32 / (tail.len() as f32 / 44100.0);
    assert!((zc - 220.0).abs() < 3.0, "{zc} Hz");
}

#[test]
fn a_linear_cycle_is_a_triangle_with_odd_harmonics_only() {
    let (x, _) = play(&points(1, &[(0.5, 1.0)], pt::LINEAR, &[]), 57, 22050);
    let tail = &x[4410..];
    let dft = |hz: f64| {
        let (mut re, mut im) = (0.0f64, 0.0f64);
        for (i, v) in tail.iter().enumerate() {
            let a = 2.0 * std::f64::consts::PI * hz * i as f64 / 44100.0;
            re += *v as f64 * a.cos();
            im += *v as f64 * a.sin();
        }
        (re * re + im * im).sqrt()
    };
    let (h1, h2, h3) = (dft(220.0), dft(440.0), dft(660.0));
    assert!(h2 / h1 < 0.03, "no even harmonic: {}", h2 / h1);
    assert!((h3 / h1 - 1.0 / 9.0).abs() < 0.03, "a triangle's third harmonic: {}", h3 / h1);
}

#[cfg(feature = "us-voice-lfo")]
#[test]
fn a_modulated_point_changes_the_cycle_between_control_segments() {
    let t = crate::modmx::target::osc(0, crate::modmx::target::OSC_SHAPE + (sp::PTS_POINT + 1) as u8);
    let moving = [lfo_1hz_v3(0), route_v3(0, crate::modmx::source::LFO1, t, 1.0)].concat();
    let (x, _) = play(&points(1, &[(0.5, 1.0)], pt::STEP, &moving), 57, 44100);
    let (y, _) = play(&points(1, &[(0.5, 1.0)], pt::STEP, &[]), 57, 44100);
    let peak_at = |v: &[f32], s: usize| v[s..s + 2205].iter().cloned().fold(f32::MIN, f32::max);
    let moved: Vec<f32> = (1..9).map(|k| peak_at(&x, k * 4410)).collect();
    assert!(moved.iter().cloned().fold(f32::MIN, f32::max) - moved.iter().cloned().fold(f32::MAX, f32::min) > 0.05, "{moved:?}");
    let still: Vec<f32> = (1..9).map(|k| peak_at(&y, k * 4410)).collect();
    assert!(still.iter().all(|p| (p - still[0]).abs() < 1e-4), "without the LFO the cycle stays: {still:?}");
}

#[test]
fn unison_layers_sum_before_the_drive() {
    // a square of ±0.5; three layers at no detune or spread add to ±1.5 before the clip, which drive 0 (×1) keeps
    let one = play(&points(1, &[(0.5, 1.0)], pt::STEP, &[]), 57, 8820).0;
    let three = play(&points(1, &[(0.5, 1.0)], pt::STEP, &[(pts(0, pt::LAYERS), inv::inv_count(3, 1, 11))]), 57, 8820).0;
    let p = |v: &[f32]| v[4410..].iter().fold(0.0f32, |m, x| m.max(x.abs()));
    // layers 2 and 3 start at the same random phase, layer 1 at 0: never more than the clip of the sum
    assert!(p(&three) > p(&one) * 1.2, "{} {}", p(&three), p(&one));
    assert!(p(&three) <= p(&one) * 2.0 + 1e-4, "the clip at ±1 bounds the sum: {} {}", p(&three), p(&one));
}

#[test]
fn a_point_slot_needs_one_cycle_per_voice() {
    let base = patch(&[]);
    let with = patch(&points(2, &[(0.3, 0.8), (0.5, 0.2)], pt::SPLINE, &[]));
    let per_voice = (UInstrument::need(&with).unwrap() - UInstrument::need(&base).unwrap()) as f64 / 16.0;
    let size = core::mem::size_of::<crate::pts::Pts>() as f64;
    assert!(per_voice >= size && per_voice < size + 64.0, "{per_voice} vs {size}");
}
