//! Elevated's voice (change add-elevated-import): the pair oscillator, the pitch drop, the linear timed envelope and
//! the monophonic hard cut.

use super::{on, render, with_ins, Ev};
use crate::law::host as inv;
use crate::params::{o, osc, osc_type, shape as sp, DEFAULTS};

/// A v3 patch with `edits` (the shared helpers work in the old layout, which has none of these fields).
fn patch(edits: &[(usize, f32)]) -> Vec<u8> {
    let mut v = DEFAULTS;
    for &(i, x) in edits {
        v[i] = x;
    }
    crate::chunk::encode(&v, &[])
}

/// Slot `i` = pair with waveform `wave`, shift `s`, detune `d`, polarity `neg`.
fn pair(i: usize, wave: u8, s: f32, d: f32, neg: bool) -> Vec<(usize, f32)> {
    let sh = |k: usize| osc(i, o::SHAPE + k);
    vec![
        (osc(i, o::TYPE), osc_type::encode(osc_type::PAIR)),
        (sh(sp::PAIR_WAVE), inv::inv_choice(wave as usize, 3)),
        (sh(sp::PAIR_SHIFT), s),
        (sh(sp::PAIR_DETUNE), d / 2.0),
        (sh(sp::PAIR_POLARITY), inv::inv_choice(neg as usize, 2)),
    ]
}

fn play(edits: &[(usize, f32)], evs: &[Ev], n: usize) -> (Vec<f32>, Vec<f32>) {
    with_ins(&patch(edits), 44100.0, |ins| render(ins, 44100.0, evs, n, 512))
}

/// Magnitude of `x` at `hz` (Hann window, Goertzel).
fn mag(x: &[f32], hz: f64) -> f64 {
    let n = x.len() as f64;
    let w = 2.0 * core::f64::consts::PI * hz / 44100.0;
    let (mut re, mut im) = (0.0, 0.0);
    for (k, &v) in x.iter().enumerate() {
        let h = 0.5 - 0.5 * (2.0 * core::f64::consts::PI * k as f64 / n).cos();
        re += v as f64 * h * (w * k as f64).cos();
        im -= v as f64 * h * (w * k as f64).sin();
    }
    (re * re + im * im).sqrt() / n
}

// ---- pair oscillator (task 2.1) ----

#[test]
fn the_pair_copies_fold_like_the_original() {
    use crate::osc::{pair_wave, pair_wave::*};
    crate::tables::init();
    // x = 2·(t − round(t)): saw 0 at t = 0, rising; −1 just after the half turn
    assert_eq!(pair_wave(0.0, SAW), 0.0);
    assert!((pair_wave(0.25, SAW) - 0.5).abs() < 1e-6);
    assert!((pair_wave(0.75, SAW) + 0.5).abs() < 1e-6);
    // square: +1 above 0, −1 at and below 0
    assert_eq!(pair_wave(0.0, SQUARE), -1.0);
    assert_eq!(pair_wave(0.25, SQUARE), 1.0);
    assert_eq!(pair_wave(0.75, SQUARE), -1.0);
    // sine: sin(πx)
    assert!((pair_wave(0.25, SINE) - 1.0).abs() < 1e-5);
    assert!((pair_wave(0.125, SINE) - 0.70710677).abs() < 1e-5);
}

#[test]
fn a_pair_at_unit_detune_is_twice_a_naive_saw_read_after_the_step() {
    let (l, _) = play(&pair(0, crate::osc::pair_wave::SAW, 0.0, 1.0, false), &[on(0, 69)], 4410);
    let inc = 440.0f32 / 44100.0;
    // the phase advances before the read: sample n reads (n + 1)·inc
    let saw = |n: usize| {
        let t = ((n + 1) as f64 * inc as f64).fract();
        2.0 * (t - t.round())
    };
    // after the attack the ratio to 2·saw is the voice gain, the same at every sample
    let ratio: Vec<f64> = (200..4410).filter(|&n| saw(n).abs() > 0.2).map(|n| l[n] as f64 / (2.0 * saw(n))).collect();
    let (lo, hi) = ratio.iter().fold((f64::MAX, f64::MIN), |(a, b), &r| (a.min(r), b.max(r)));
    assert!(lo > 0.0 && hi - lo < 1e-3 * hi, "constant gain: {lo} .. {hi}");
}

#[test]
fn a_detuned_pair_peaks_at_both_ratios() {
    let (l, _) = play(&pair(0, crate::osc::pair_wave::SINE, 0.0, 0.99, false), &[on(0, 69)], 44100 * 2);
    let x = &l[4410..];
    let (a, b, mid) = (mag(x, 440.0 * 0.99), mag(x, 440.0 * 1.01), mag(x, 440.0));
    assert!(a > 20.0 * mid && b > 20.0 * mid, "435.6 Hz {a}, 444.4 Hz {b}, 440 Hz {mid}");
    assert!((a / b - 1.0).abs() < 0.05, "equal copies: {a} {b}");
}

#[test]
fn a_negated_pair_subtracts_from_the_sum() {
    let mut e = pair(0, crate::osc::pair_wave::SQUARE, 0.13, 1.01, false);
    e.extend(pair(1, crate::osc::pair_wave::SQUARE, 0.13, 1.01, true));
    let (l, r) = play(&e, &[on(0, 60)], 8820);
    let peak = l.iter().chain(&r).fold(0.0f32, |m, v| m.max(v.abs()));
    assert!(peak < 1e-6, "below −120 dBFS: {peak}");
    // and one slot alone sounds
    let (l1, _) = play(&pair(0, crate::osc::pair_wave::SQUARE, 0.13, 1.01, false), &[on(0, 60)], 8820);
    assert!(l1.iter().fold(0.0f32, |m, v| m.max(v.abs())) > 0.1);
}

#[test]
fn the_pair_restarts_from_phase_zero_at_every_note() {
    // capacity 1, steal oldest: the second note restarts the same voice, whose phases start again at 0
    let mut e = pair(0, crate::osc::pair_wave::SAW, 0.0, 1.0, false);
    e.push((crate::params::v::CAPACITY, inv::inv_count(1, 1, 64)));
    let (a, _) = play(&e, &[on(0, 69)], 600);
    let (b, _) = play(&e, &[on(0, 64), on(1000, 69)], 1600);
    for k in 0..600 {
        assert!((b[1000 + k] - a[k]).abs() < 1e-6, "frame {k}: {} vs {}", b[1000 + k], a[k]);
    }
}

// ---- monophonic hard cut (task 2.4) ----

#[test]
fn a_mono_instrument_cuts_the_old_note_at_the_new_one() {
    use crate::params::{e, env, v};
    // capacity 1, steal oldest (the default), envelope 1 with a 1 s release: the second note does not let the first
    // ring out, the output from its frame is the second note alone
    let mut ed = pair(0, crate::osc::pair_wave::SINE, 0.0, 1.0, false);
    ed.push((v::CAPACITY, inv::inv_count(1, 1, 64)));
    ed.push((env(0, e::RELEASE), inv::inv_env_ms(1000.0)));
    let (both, _) = play(&ed, &[on(0, 57), on(22050, 64)], 22050 + 8820);
    let (alone, _) = play(&ed, &[on(22050, 64)], 22050 + 8820);
    for k in 22050..22050 + 8820 {
        assert!((both[k] - alone[k]).abs() < 1e-6, "frame {k}: {} vs {}", both[k], alone[k]);
    }
}

// ---- pitch drop (task 2.3) ----

#[test]
fn a_drop_voice_follows_the_closed_form_per_sample() {
    use crate::params::el;
    // note 45 (110 Hz) towards 40 Hz at k = 0.9993 (Atrium's kick), a sine pair at d = 1: 2·sin(2πφ)
    let k = 0.9993f64;
    let mut ed = pair(0, crate::osc::pair_wave::SINE, 0.0, 1.0, false);
    ed.push((el::DROP_BASE, inv::inv_drop_hz(40.0)));
    ed.push((el::DROP_RATE, inv::inv_drop_k(k)));
    let (l, _) = play(&ed, &[on(0, 45)], 8820);
    let mut phase = 0.0f64;
    let want: Vec<f64> = (0..8820)
        .map(|n| {
            phase += (40.0 + 70.0 * k.powi(n as i32 + 1)) / 44100.0;
            2.0 * (2.0 * core::f64::consts::PI * phase).sin()
        })
        .collect();
    // after the 1 ms attack the output is the closed form times the voice gain
    let (a, b) = (&l[100..], &want[100..]);
    let dot: f64 = a.iter().zip(b).map(|(x, y)| *x as f64 * y).sum();
    let na: f64 = a.iter().map(|x| (*x as f64).powi(2)).sum();
    let nb: f64 = b.iter().map(|y| y * y).sum();
    let corr = dot / (na * nb).sqrt();
    assert!(corr > 0.9999, "correlation {corr}");
}

#[test]
#[cfg(feature = "std")] // `needs` is host-only
fn a_drop_rate_of_zero_is_off_and_needs_no_module() {
    use crate::params::el;
    let mut ed = pair(0, crate::osc::pair_wave::SINE, 0.0, 1.0, false);
    ed.push((el::DROP_BASE, inv::inv_drop_hz(40.0)));
    let plain = play(&pair(0, crate::osc::pair_wave::SINE, 0.0, 1.0, false), &[on(0, 45)], 4410);
    assert_eq!(play(&ed, &[on(0, 45)], 4410), plain, "a base without a rate changes nothing");
    let c = patch(&ed);
    let mods = crate::needs::instrument(&c, &|_| None).unwrap();
    assert!(!mods.contains(&crate::module::VOICE_DROP));
    ed.push((el::DROP_RATE, inv::inv_drop_k(0.9993)));
    assert!(crate::needs::instrument(&patch(&ed), &|_| None).unwrap().contains(&crate::module::VOICE_DROP));
}

// ---- linear timed envelope (task 2.5) ----

/// Envelope 1 linear: attack, decay, hold and release in frames at 44.1 kHz, sustain `s`.
fn linear_env(a: f32, d: f32, h: f32, r: f32, s: f32) -> Vec<(usize, f32)> {
    use crate::params::{e, el, el_env, env};
    let ms = |frames: f32| inv::inv_lin_ms(frames / 44.1);
    vec![
        (el_env(0, el::LINEAR), 1.0),
        (env(0, e::ATTACK), ms(a)),
        (env(0, e::DECAY), ms(d)),
        (el_env(0, el::HOLD), ms(h)),
        (env(0, e::RELEASE), ms(r)),
        (env(0, e::SUSTAIN), s),
    ]
}

#[test]
fn a_linear_envelope_runs_its_four_stages_by_itself() {
    // a square pair at d = 1 is ±2 at every sample: |output| traces the envelope
    let mut ed = pair(0, crate::osc::pair_wave::SQUARE, 0.0, 1.0, false);
    ed.extend(linear_env(1000.0, 2000.0, 2000.0, 4000.0, 0.5));
    let (l, _) = play(&ed, &[on(0, 60)], 12000);
    let peak = l[1000].abs();
    let lvl = |k: usize| l[k].abs() / peak;
    // the segments end at the stage corners, so the level is the linear law at every frame
    for (k, want) in [(500, 0.5), (999, 0.999), (2000, 0.75), (2999, 0.50025), (3000, 0.5), (4000, 0.5), (5000, 0.5), (7000, 0.25)] {
        assert!((lvl(k) - want).abs() < 2e-3, "frame {k}: {} vs {want}", lvl(k));
    }
    assert!(l[9100..].iter().all(|v| *v == 0.0), "ends by itself after 9000 frames, no note-off");
}

#[test]
fn a_note_off_cuts_a_linear_envelope() {
    let mut ed = pair(0, crate::osc::pair_wave::SQUARE, 0.0, 1.0, false);
    ed.extend(linear_env(10.0, 500.0, 20000.0, 20000.0, 0.5));
    let (l, _) = play(&ed, &[on(0, 60), super::off(3500, 60)], 6000);
    assert!(l[3400].abs() > 0.1, "sounding in the hold");
    assert!(l[3500 + 32..].iter().all(|v| *v == 0.0), "silent within 32 frames of the note-off");
}

#[test]
#[cfg(feature = "std")] // `needs` is host-only
fn a_linear_envelope_needs_its_module() {
    use crate::params::{el, el_env};
    let mut ed = pair(0, crate::osc::pair_wave::SQUARE, 0.0, 1.0, false);
    assert!(!crate::needs::instrument(&patch(&ed), &|_| None).unwrap().contains(&crate::module::ENV_LINEAR));
    ed.push((el_env(0, el::LINEAR), 1.0));
    assert!(crate::needs::instrument(&patch(&ed), &|_| None).unwrap().contains(&crate::module::ENV_LINEAR));
}

#[test]
fn a_three_frame_attack_keeps_its_corner() {
    // Elevated's kick: attack 3 frames, then a long decay; the corner is at frame 3, not smeared over a control segment
    let mut ed = pair(0, crate::osc::pair_wave::SQUARE, 0.0, 1.0, false);
    ed.extend(linear_env(3.0, 512.0, 4992.0, 6912.0, 0.5));
    let (l, _) = play(&ed, &[on(0, 60)], 64);
    let peak = l[3].abs();
    assert!((l[1].abs() / peak - 1.0 / 3.0).abs() < 2e-3 && (l[2].abs() / peak - 2.0 / 3.0).abs() < 2e-3, "{:?}", &l[..4]);
    assert!((l[8].abs() / peak - (1.0 - 0.5 * 5.0 / 512.0)).abs() < 2e-3, "decaying from the corner: {}", l[8].abs() / peak);
}
