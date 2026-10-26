//! The band-limited saw/pulse (Slaughter's integrated BLIT).

use super::*;
use crate::params::shape;
use synth_core::voice::VoiceState;

fn dft(x: &[f32], from: usize, n: usize, hz: f64, rate: f64) -> f64 {
    let (mut re, mut im) = (0.0f64, 0.0f64);
    for (i, v) in x[from..from + n].iter().enumerate() {
        let a = 2.0 * std::f64::consts::PI * hz * (from + i) as f64 / rate;
        re += *v as f64 * a.cos();
        im += *v as f64 * a.sin();
    }
    2.0 * (re * re + im * im).sqrt() / n as f64
}

fn blit_slot(i: usize) -> Vec<(usize, f32)> {
    vec![(osc(i, o::TYPE), inv::inv_choice(osc_type::BLIT, osc_type::COUNT))]
}

/// Edits that turn slot 0 off and `slot` into a saw/pulse, tuned `semis` above `note` plus a small fine offset.
fn edits(extra: &[(usize, f32)]) -> Vec<(usize, f32)> {
    let mut e = vec![(osc(0, o::TYPE), inv::inv_choice(osc_type::OFF, osc_type::COUNT))];
    e.extend(blit_slot(1));
    e.extend_from_slice(extra);
    e
}

/// Note 45 is 110 Hz; a quarter-hertz fine offset makes the period exactly 400 samples at 44.1 kHz.
fn fine_110_25() -> (usize, f32) {
    (osc(1, o::FINE), inv::inv_fine(12.0 * (110.25f32 / 110.0).log2()))
}

fn play(extra: &[(usize, f32)], note: u8, rate: f64, n: usize) -> Vec<f32> {
    with_ins(&patch(&edits(extra)), rate, |ins| render(ins, rate, &[on(0, note)], n, 4096).0)
}

const F0: f64 = 110.25;

#[test]
fn the_saw_spectrum_falls_off_as_one_over_n() {
    let x = play(&[fine_110_25()], 45, 44100.0, 44100);
    let (from, n) = (4000, 40000); // 100 periods
    let h = |k: u32| dft(&x, from, n, F0 * k as f64, 44100.0);
    let h1 = h(1);
    assert!(h1 > 0.05, "{h1}");
    // every harmonic below 5 kHz (n ≤ 45): amplitude · n stays at the fundamental's within 8 %
    for k in 2..=45u32 {
        let r = h(k) * k as f64 / h1;
        assert!((r - 1.0).abs() < 0.08, "harmonic {k}: {r}");
    }
}

#[test]
fn the_saw_has_no_dc_and_is_not_inverted_against_slaughter() {
    let x = play(&[fine_110_25()], 45, 44100.0, 44100);
    let mean = x[4000..44000].iter().sum::<f32>() / 40000.0;
    assert!(mean.abs() < 0.01 * peak(&x), "dc {mean}");
    // Slaughter's saw (negated by its −16 output gain) rises slowly and drops fast: the steepest step is downward
    let w = &x[4000..44000];
    let (up, down) = w.windows(2).fold((0.0f32, 0.0f32), |(u, d), p| (u.max(p[1] - p[0]), d.max(p[0] - p[1])));
    assert!(up < down, "up {up}, down {down}");
}

#[test]
fn a_square_has_only_odd_harmonics_and_a_quarter_pulse_lacks_the_fourth() {
    let x = play(&[fine_110_25(), (osc(1, o::SHAPE + shape::BLIT_WAVEFORM), 1.0)], 45, 44100.0, 44100);
    let h = |x: &[f32], k: u32| dft(x, 4000, 40000, F0 * k as f64, 44100.0);
    assert!(h(&x, 1) > 0.05);
    for k in [2u32, 4, 6, 8] {
        assert!(h(&x, k) / h(&x, 1) < 0.03, "even harmonic {k}: {}", h(&x, k) / h(&x, 1));
    }
    assert!((h(&x, 3) / h(&x, 1) - 1.0 / 3.0).abs() < 0.03);
    let q = play(&[fine_110_25(), (osc(1, o::SHAPE + shape::BLIT_WAVEFORM), 1.0), (osc(1, o::SHAPE + shape::BLIT_PULSE_WIDTH), 0.25)], 45, 44100.0, 44100);
    assert!(h(&q, 4) / h(&q, 1) < 0.03 && h(&q, 8) / h(&q, 1) < 0.03, "{}", h(&q, 4) / h(&q, 1));
    assert!(h(&q, 2) / h(&q, 1) > 0.5, "a quarter pulse has a second harmonic");
}

#[test]
fn each_slot_has_its_own_pitch() {
    // note 69 at 441 Hz: slot 1 at ratio 1, slot 2 at ratio 2, one alone at a time
    let f = (osc(1, o::FINE), inv::inv_fine(12.0 * (441.0f32 / 440.0).log2()));
    let g = (osc(2, o::FINE), f.1);
    let slot2 = |ratio: f32, mix1: f32| {
        let mut e = vec![f, g, (osc(2, o::TYPE), inv::inv_choice(osc_type::BLIT, osc_type::COUNT)), (osc(2, o::RATIO), inv::inv_ratio(ratio)), (osc(1, o::MIX), mix1)];
        e.push((osc(2, o::MIX), 1.0));
        play(&e, 69, 44100.0, 44100)
    };
    let h = |x: &[f32], hz: f64| dft(x, 4000, 39000, hz, 44100.0);
    // slot 1 muted: slot 2 alone at ratio 2 has its fundamental at 882 Hz and nothing at 441
    let only2 = slot2(2.0, 0.0);
    assert!(h(&only2, 882.0) > 0.05 && h(&only2, 441.0) < 0.002, "{} {}", h(&only2, 882.0), h(&only2, 441.0));
    // slot 2 muted: slot 1 alone has its fundamental at 441 and the harmonic series
    let mut e = vec![f, (osc(2, o::TYPE), inv::inv_choice(osc_type::BLIT, osc_type::COUNT)), (osc(2, o::MIX), 0.0)];
    e.push((osc(2, o::RATIO), inv::inv_ratio(2.0)));
    let only1 = play(&e, 69, 44100.0, 44100);
    assert!(h(&only1, 441.0) > 0.05);
}

#[test]
fn every_slot_and_noise_off_is_silent() {
    let x = with_ins(&patch(&[(osc(0, o::TYPE), inv::inv_choice(osc_type::OFF, osc_type::COUNT)), (osc(1, o::TYPE), inv::inv_choice(osc_type::OFF, osc_type::COUNT))]), 44100.0, |ins| render(ins, 44100.0, &[on(0, 60)], 4000, 1000).0);
    assert_eq!(peak(&x), 0.0);
}

#[test]
fn pitch_tracks_the_rate() {
    for rate in [22050.0, 44100.0, 48000.0, 96000.0] {
        let x = play(&[], 57, rate, rate as usize); // 220 Hz
        let w = &x[rate as usize / 5..];
        let c = crossings(w) as f64 / (w.len() as f64 / rate);
        assert!((c - 220.0).abs() < 2.0, "{rate}: {c} crossings/s");
    }
}

#[test]
fn the_leak_keeps_its_corner_at_every_rate() {
    // note 21 (27.5 Hz) sits next to the integrator's corner: its level shows the corner, and it must not move with
    // the rate. The window holds whole periods (2 s).
    let level = |rate: f64| {
        let x = play(&[], 21, rate, 3 * rate as usize);
        dft(&x, rate as usize, 2 * rate as usize, 27.5, rate)
    };
    let base = level(44100.0);
    for rate in [22050.0, 48000.0, 96000.0] {
        assert!((level(rate) / base - 1.0).abs() < 0.02, "{rate}: {} vs {base}", level(rate));
    }
}

#[test]
fn phase_and_integrator_run_on_across_notes_but_the_pm_operator_restarts() {
    let mut e = edits(&[]);
    e.extend([(osc(2, o::TYPE), inv::inv_choice(osc_type::PM, osc_type::COUNT)), (pm::v::ENV1_AT_OUTPUT, 1.0)]);
    with_ins(&patch(&e), 44100.0, |ins| {
        render(ins, 44100.0, &[on(0, 60)], 3000, 1000);
        ins.all_notes_off();
        render_more(ins, &[], 3000, 220500); // the release runs out
        assert!(!ins.voices()[0].is_on());
        let (saw, pmod) = (ins.voices()[0].slot_phase(1), ins.voices()[0].slot_phase(2));
        render_more(ins, &[], 223500, 1000);
        assert_eq!(ins.voices()[0].slot_phase(1), saw, "an idle voice keeps its phase");
        render_more(ins, &[on(224500, 60)], 224500, 1);
        let (saw2, pmod2) = (ins.voices()[0].slot_phase(1), ins.voices()[0].slot_phase(2));
        assert!((saw2 - saw).abs() < 0.02, "the saw runs on: {saw} -> {saw2}");
        assert!((pmod2 - pmod).abs() > 0.02, "the phase-mod operator restarts: {pmod} -> {pmod2}");
    });
}

#[test]
fn voices_and_slots_start_from_different_phases_and_reset_repeats_them() {
    let e = edits(&[]);
    with_ins(&patch(&e), 44100.0, |ins| {
        let ph: Vec<f64> = ins.voices().iter().map(|v| v.slot_phase(1)).collect();
        assert!(ph.windows(2).all(|w| w[0] != w[1]));
        assert!(ins.voices()[0].slot_phase(1) != ins.voices()[0].slot_phase(2));
        ins.reset();
        let again: Vec<f64> = ins.voices().iter().map(|v| v.slot_phase(1)).collect();
        assert_eq!(ph, again);
    });
}

#[test]
fn the_modulation_targets_move_waveform_and_pulse_width() {
    // waveform 0 (saw) with an Offset route of +1 on the waveform shape value gives a square: no even harmonics
    let e = route(0, source::VELOCITY, target::osc(1, target::OSC_SHAPE + shape::BLIT_WAVEFORM as u8), false, 1.0);
    let mut all = edits(&[fine_110_25()]);
    all.extend(e.into_iter());
    let x = with_ins(&patch(&all), 44100.0, |ins| render(ins, 44100.0, &[Ev { frame: 0, note: 45, vel: 127 }], 44100, 4096).0);
    let h = |k: u32| dft(&x, 6000, 38000, F0 * k as f64, 44100.0);
    assert!(h(1) > 0.05 && h(2) / h(1) < 0.08, "{}", h(2) / h(1));
}

// ---- the start phase spread (voice parameter `start_spread`) ----

/// RMS of the first 6000 frames of a unison group of `n` voices at one pitch, for the start-phase spread `p`,
/// relative to one voice. In phase the voices add up to `n`; with phases spread over the turn they add up to about √n.
fn unison_gain(slot_type: usize, p: f32, note: u8, voices: usize) -> f32 {
    let mut e = vec![(osc(0, o::TYPE), inv::inv_choice(slot_type, osc_type::COUNT)), (v::START_SPREAD, p)];
    if slot_type == osc_type::BLIT {
        e.push((osc(0, o::SHAPE + shape::BLIT_WAVEFORM), 0.0));
    }
    e.push((v::UNISON, inv::inv_count(voices as u32, 1, 16)));
    e.push((v::UNI_SPREAD, 0.5));
    let x = with_ins(&patch(&e), 44100.0, |ins| render(ins, 44100.0, &[on(0, note)], 6000, 300).0);
    let one = {
        let mut e1 = e.clone();
        e1.retain(|(i, _)| *i != v::UNISON);
        with_ins(&patch(&e1), 44100.0, |ins| render(ins, 44100.0, &[on(0, note)], 6000, 300).0)
    };
    let rms = |x: &[f32]| (x[1000..].iter().map(|v| (*v as f64).powi(2)).sum::<f64>() / (x.len() - 1000) as f64).sqrt();
    (rms(&x) / rms(&one)) as f32
}

#[test]
fn a_saw_unison_group_starts_almost_in_phase_when_the_spread_is_small() {
    // Slaughter draws its start phase in samples, not turns: about ±6 samples of a 675-sample period at note 36
    let original = inv::inv_start_spread(12.566);
    let near = unison_gain(osc_type::BLIT, original, 36, 4);
    let full = unison_gain(osc_type::BLIT, 1.0, 36, 4);
    assert!(near > 3.6, "four voices almost in phase add up to almost four: {near}");
    assert!(full < 3.0, "…and with the whole turn they do not: {full}");
}

#[test]
fn the_spread_is_a_number_of_samples_so_high_notes_spread_over_more_of_their_period() {
    let original = inv::inv_start_spread(12.566);
    // note 84 (1047 Hz, a 42-sample period): 12.6 samples is 30 % of the turn, the sum is clearly below in phase
    let high = unison_gain(osc_type::BLIT, original, 84, 4);
    let low = unison_gain(osc_type::BLIT, original, 36, 4);
    assert!(high < low - 0.2, "{high} vs {low}");
}

#[test]
fn a_phase_mod_unison_group_starts_in_phase_at_the_minimum_and_not_at_the_default() {
    let in_phase = unison_gain(osc_type::PM, 0.0, 60, 4);
    let spread = unison_gain(osc_type::PM, 1.0, 60, 4);
    assert!(in_phase > 3.95, "{in_phase}");
    assert!(spread < 3.5, "{spread}");
}

#[test]
fn the_default_spread_is_the_whole_turn_and_the_law_has_its_two_ends() {
    assert_eq!(pm::DEFAULTS[v::START_SPREAD], 1.0);
    assert!((crate::law::start_spread(0.0) - 0.01).abs() < 1e-6);
    assert!((crate::law::start_spread(1.0) - 10_000.0).abs() < 1.0);
    assert_eq!(crate::law::start_phase(0.7, 1.0, 1e-9), 0.7, "p = 1 is the whole turn at any pitch");
    assert!((crate::law::start_phase(0.5, inv::inv_start_spread(100.0), 0.001) - 0.05).abs() < 1e-3);
    assert!(crate::law::start_phase(0.99, inv::inv_start_spread(5000.0), 0.01) <= 0.99);
    let p = inv::inv_start_spread(12.566);
    assert!((crate::law::start_spread(p) - 12.566).abs() < 0.01);
}

#[test]
fn only_the_first_note_of_a_saw_voice_takes_the_spread() {
    // the integrator and phase run on across notes (Slaughter), so a second note must not rescale the phase
    let e = vec![
        (osc(0, o::TYPE), inv::inv_choice(osc_type::BLIT, osc_type::COUNT)),
        (v::START_SPREAD, inv::inv_start_spread(12.566)),
    ];
    let twice = with_ins(&patch(&e), 44100.0, |ins| {
        render(ins, 44100.0, &[on(0, 45), off(4000, 45), on(8000, 45), off(12000, 45)], 14000, 500).0
    });
    let again = with_ins(&patch(&e), 44100.0, |ins| {
        render(ins, 44100.0, &[on(0, 45), off(4000, 45), on(8000, 45), off(12000, 45)], 14000, 77).0
    });
    assert_eq!(twice, again, "independent of the call size");
    assert!(peak(&twice) > 0.05);
}
