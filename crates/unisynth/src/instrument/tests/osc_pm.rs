//! The phase-mod operator (Falcon): sine + Square35, feedback, sends, ratios.

use super::*;
use crate::params::old::shape;

/// Magnitude of the DFT bin at `hz` over `n` frames of `x` starting at `from` (rate 44 100).
fn mag(x: &[f32], from: usize, n: usize, hz: f64) -> f64 {
    let (mut re, mut im) = (0.0f64, 0.0f64);
    for (i, v) in x[from..from + n].iter().enumerate() {
        let a = 2.0 * std::f64::consts::PI * hz * (from + i) as f64 / 44100.0;
        re += *v as f64 * a.cos();
        im += *v as f64 * a.sin();
    }
    2.0 * (re * re + im * im).sqrt() / n as f64
}

/// Edits for slot `i` as a phase-mod operator.
fn pm_slot(i: usize) -> Vec<(usize, f32)> {
    vec![(osc(i, o::TYPE), inv::inv_choice(osc_type::PM, osc_type::COUNT))]
}

/// Renders 1 s of note 69 shifted to exactly 441 Hz (100 samples per period at 44.1 kHz).
fn play(edits: &[(usize, f32)]) -> Vec<f32> {
    let mut e = vec![(osc(0, o::FINE), inv::inv_fine(12.0 * (441.0f32 / 440.0).log2()))];
    e.extend_from_slice(edits);
    with_ins(&patch(&e), 44100.0, |ins| render(ins, 44100.0, &[on(0, 69)], 44100, 4096).0)
}

const F0: f64 = 441.0;
const FROM: usize = 4400;
const N: usize = 22000; // 220 periods

fn harmonic(x: &[f32], k: u32) -> f64 {
    mag(x, FROM, N, F0 * k as f64)
}

#[test]
fn feedback_free_blend_free_operator_is_a_pure_sine() {
    let x = play(&[]);
    let (h1, h2, h3, h5) = (harmonic(&x, 1), harmonic(&x, 2), harmonic(&x, 3), harmonic(&x, 5));
    assert!((h1 - 0.5f64.sqrt() * 100.0 / 128.0).abs() < 0.01, "{h1}");
    assert!(h2 / h1 < 1e-3 && h3 / h1 < 1e-3 && h5 / h1 < 1e-3, "{h2} {h3} {h5}");
}

#[test]
fn the_waveform_parameter_blends_in_square35() {
    // sin + wf·(sin 3φ/3 + sin 5φ/5): harmonics 3 and 5 at 1/3 and 1/5 of the fundamental
    let x = play(&[(osc(0, o::SHAPE + shape::PM_WAVEFORM), 1.0)]);
    let h1 = harmonic(&x, 1);
    assert!((harmonic(&x, 3) / h1 - 1.0 / 3.0).abs() < 0.01, "{}", harmonic(&x, 3) / h1);
    assert!((harmonic(&x, 5) / h1 - 0.2).abs() < 0.01);
    assert!(harmonic(&x, 2) / h1 < 1e-3 && harmonic(&x, 4) / h1 < 1e-3);
    let half = play(&[(osc(0, o::SHAPE + shape::PM_WAVEFORM), 0.5)]);
    assert!((harmonic(&half, 3) / harmonic(&half, 1) - 1.0 / 6.0).abs() < 0.01);
}

/// Reference two-operator FM: `sin(θ + β·sin θ)`, harmonic `m` magnitude over whole periods.
fn fm_reference(beta: f64, m: u32) -> f64 {
    let n = 22000;
    let (mut re, mut im) = (0.0, 0.0);
    for i in 0..n {
        let th = 2.0 * std::f64::consts::PI * F0 * i as f64 / 44100.0;
        let y = (th + beta * th.sin()).sin();
        let a = th * m as f64;
        re += y * a.cos();
        im += y * a.sin();
    }
    2.0 * (re * re + im * im).sqrt() / n as f64
}

#[test]
fn two_operator_fm_has_the_bessel_sidebands() {
    // slot 1 modulates slot 2 (ratio 1 each): mix 0 on the modulator, send 4p² = 2 radians per unit of output
    let send_p = inv::inv_pm_out(2.0);
    let mut e = pm_slot(1);
    e.extend([(osc(0, o::MIX), 0.0), (osc(0, o::PM_OUT), send_p), (osc(1, o::FINE), inv::inv_fine(12.0 * (441.0f32 / 440.0).log2()))]);
    let x = play(&e);
    let level = harmonic(&x, 1) + harmonic(&x, 2) + harmonic(&x, 3);
    assert!(level > 0.1);
    let out = 0.5f64.sqrt() * 100.0 / 128.0; // pan and velocity of the carrier path
    for m in 1..=4 {
        let want = fm_reference(2.0, m) * out;
        assert!((harmonic(&x, m) - want).abs() < 0.01, "harmonic {m}: {} vs {want}", harmonic(&x, m));
    }
    // send 0: the modulator no longer matters; the carrier is a pure sine
    let mut e = pm_slot(1);
    e.extend([(osc(0, o::MIX), 0.0), (osc(0, o::PM_OUT), 0.0), (osc(1, o::FINE), inv::inv_fine(12.0 * (441.0f32 / 440.0).log2()))]);
    let x = play(&e);
    assert!(harmonic(&x, 2) / harmonic(&x, 1) < 1e-3);
}

#[test]
fn feedback_adds_harmonics() {
    let none = play(&[]);
    let fb = play(&[(osc(0, o::FEEDBACK), inv::inv_feedback(1.5))]);
    assert!(harmonic(&fb, 2) / harmonic(&fb, 1) > 0.1, "{}", harmonic(&fb, 2) / harmonic(&fb, 1));
    assert!(harmonic(&none, 2) / harmonic(&none, 1) < 1e-3);
    // feedback ramps in by one sample: no explosion at the largest setting
    let wild = play(&[(osc(0, o::FEEDBACK), 1.0)]);
    assert!(wild.iter().all(|v| v.is_finite() && v.abs() < 4.0));
}

#[test]
fn ratio_and_ratio_fine_follow_falcon() {
    // ratio 3 (1 + floor(p·32.99) = 3) puts the fundamental on 3 × the note; ratio fine adds (2p − 1)³
    let x = play(&[(osc(0, o::RATIO), inv::inv_ratio(3.0))]);
    assert!(harmonic(&x, 3) > 0.5 && harmonic(&x, 1) < 0.01);
    let x = play(&[(osc(0, o::RATIO), inv::inv_ratio(2.0)), (osc(0, o::RATIO_FINE), 1.0)]);
    assert!(harmonic(&x, 3) > 0.5, "ratio 2 + fine 1 = 3");
}

#[test]
fn slot_gain_mix_and_octave_apply() {
    let base = harmonic(&play(&[]), 1);
    let loud = harmonic(&play(&[(osc(0, o::GAIN), inv::inv_gain(2.0))]), 1);
    assert!((loud / base - 2.0).abs() < 0.02);
    let half = harmonic(&play(&[(osc(0, o::MIX), 0.5)]), 1);
    assert!((half / base - 0.5).abs() < 0.02);
    let up = play(&[(osc(0, o::OCTAVE), inv::inv_octave(1.0))]);
    assert!(harmonic(&up, 2) > 0.5 && harmonic(&up, 1) < 0.01);
    let fixed = play(&[(osc(0, o::TRACKING), 1.0), (osc(0, o::FIXED_HZ), inv::inv_fixed_hz(882.0))]);
    assert!(harmonic(&fixed, 2) > 0.5 && harmonic(&fixed, 1) < 0.01, "a fixed 882 Hz ignores the note");
}
