//! The spectral wavetable oscillator (Tunefish generator): spectrum, unison, drive, rebuild cadence, rates.

use super::*;
use crate::params::shape as sp;

fn dft(x: &[f32], from: usize, n: usize, hz: f64, rate: f64) -> f64 {
    let (mut re, mut im) = (0.0f64, 0.0f64);
    for (i, v) in x[from..from + n].iter().enumerate() {
        let a = 2.0 * std::f64::consts::PI * hz * (from + i) as f64 / rate;
        re += *v as f64 * a.cos();
        im += *v as f64 * a.sin();
    }
    2.0 * (re * re + im * im).sqrt() / n as f64
}

fn sh(k: usize) -> usize {
    osc(0, o::SHAPE + k)
}

/// Slot 1 = wavetable with one partial at bin 2 (scale 1 puts partial h at bin 2h), no unison, no drive, no detune.
/// The cycle then holds two periods, so the sound is at twice the note's frequency.
fn wt(extra: &[(usize, f32)]) -> Vec<(usize, f32)> {
    let mut e = vec![
        (osc(0, o::TYPE), inv::inv_choice(osc_type::WAVETABLE, osc_type::COUNT)),
        (sh(sp::WT_HARMONICS), 0.0),
        (sh(sp::WT_SCALE), 0.25),
        (sh(sp::WT_LAYERS), 0.0),
        (sh(sp::WT_DRIVE), 0.0),
    ];
    e.extend_from_slice(extra);
    e
}

/// Fine tuning that turns note 69 into exactly 441 Hz.
fn fine441() -> (usize, f32) {
    (osc(0, o::FINE), inv::inv_fine(12.0 * (441.0f32 / 440.0).log2()))
}

fn play(extra: &[(usize, f32)], note: u8, rate: f64, n: usize) -> (Vec<f32>, Vec<f32>) {
    with_ins(&patch(&wt(extra)), rate, |ins| render(ins, rate, &[on(0, note)], n, 4096))
}

#[test]
fn one_partial_at_bin_two_sounds_at_twice_the_note() {
    let (x, _) = play(&[fine441()], 69, 44100.0, 44100);
    let h = |hz: f64| dft(&x, 4000, 39000, hz, 44100.0);
    assert!(h(882.0) > 0.05, "{}", h(882.0));
    assert!(h(441.0) / h(882.0) < 0.08, "the note itself: {}", h(441.0) / h(882.0));
    assert!(h(1323.0) / h(882.0) < 0.08);
}

#[test]
fn a_partial_series_has_the_harmonics_of_the_damp_law() {
    // partials at bins 2, 4, 6, … with gain 1/h (damp 0): a sawtooth at 882 Hz; width 0.3 bins keeps them apart
    let (x, _) = play(&[fine441(), (sh(sp::WT_HARMONICS), 1.0), (sh(sp::WT_DAMP), 0.0)], 69, 44100.0, 44100);
    let h = |k: u32| dft(&x, 4000, 39000, 882.0 * k as f64, 44100.0);
    for k in 2..=5u32 {
        let r = h(k) * k as f64 / h(1);
        assert!((r - 1.0).abs() < 0.15, "harmonic {k}: {r}");
    }
    assert!(dft(&x, 4000, 39000, 441.0, 44100.0) / h(1) < 0.08, "odd multiples of the note are absent");
    // damp 1 steepens the series to 1/h²
    let (y, _) = play(&[fine441(), (sh(sp::WT_HARMONICS), 1.0), (sh(sp::WT_DAMP), 1.0)], 69, 44100.0, 44100);
    let g = |k: u32| dft(&y, 4000, 39000, 882.0 * k as f64, 44100.0);
    assert!((g(2) / g(1) - 0.25).abs() < 0.06, "{}", g(2) / g(1));
}

#[test]
fn the_same_pitch_at_every_rate() {
    for rate in [22050.0, 44100.0, 48000.0, 96000.0] {
        let (x, _) = play(&[], 57, rate, rate as usize); // 220 Hz × 2 table periods
        let w = &x[rate as usize / 5..];
        let c = crossings(w) as f64 / (w.len() as f64 / rate);
        assert!((c - 440.0).abs() < 5.0, "{rate}: {c} crossings/s");
    }
}

#[test]
fn detune_splits_left_and_right_and_spread_spaces_the_layers() {
    // detune p = 1: ±10 Hz of table rate, so the left channel sounds at 2(441 + 10) and the right at 2(441 − 10)
    let (l, r) = play(&[fine441(), (sh(sp::WT_LAYER_DETUNE), 1.0)], 69, 44100.0, 44100);
    let cl = crossings(&l[4000..]) as f64 / ((l.len() - 4000) as f64 / 44100.0);
    let cr = crossings(&r[4000..]) as f64 / ((r.len() - 4000) as f64 / 44100.0);
    assert!((cl - 902.0).abs() < 4.0 && (cr - 862.0).abs() < 4.0, "{cl} {cr}");
    // detune follows 10p³ Hz: p = ½ is 1.25 Hz, so left and right differ by 4 · 1.25 = 5 Hz in the sound
    let (l, r) = play(&[fine441(), (sh(sp::WT_LAYER_DETUNE), 0.5)], 69, 44100.0, 44100);
    let cl = crossings(&l[4000..]) as f64 / ((l.len() - 4000) as f64 / 44100.0);
    let cr = crossings(&r[4000..]) as f64 / ((r.len() - 4000) as f64 / 44100.0);
    assert!((cl - cr - 5.0).abs() < 1.6, "{cl} {cr}");
    // ten layers 0.7⁴·10 = 2.4 Hz apart (4.8 Hz in the sound): ten lines, the eleventh slot empty
    for rate in [44100.0, 96000.0] {
        let layers = [(sh(sp::WT_LAYERS), 1.0), (sh(sp::WT_LAYER_SPREAD), 0.7)];
        let (l, _) = with_ins(&patch(&wt(&[fine441(), layers[0], layers[1]])), rate, |ins| render(ins, rate, &[on(0, 69)], 3 * rate as usize, 4096));
        let spacing = 2.0 * 10.0 * 0.7f64.powi(4);
        let line = |k: i32| dft(&l, 10000, 2 * rate as usize, 882.0 + spacing * k as f64, rate);
        let top = (0..10).map(|k| line(k)).fold(0.0f64, f64::max);
        for k in 0..10 {
            assert!(line(k) > 0.25 * top, "{rate}: layer {k}: {} of {top}", line(k));
        }
        assert!(line(10) < 0.1 * top && line(-1) < 0.1 * top, "{rate}: {} {}", line(10), line(-1));
    }
}

#[test]
fn a_pitch_route_scales_the_detune_with_the_frequency() {
    // velocity (100/128) × 2 = ×1.5625 on every oscillator's frequency, detune included (Tunefish multiplies both)
    let e = merged(&[vec![fine441(), (sh(sp::WT_LAYER_DETUNE), 1.0)], route(0, source::VELOCITY, target::PITCH, true, 2.0)]);
    let (l, r) = play(&e, 69, 44100.0, 44100);
    let cl = crossings(&l[4000..]) as f64 / ((l.len() - 4000) as f64 / 44100.0);
    let cr = crossings(&r[4000..]) as f64 / ((r.len() - 4000) as f64 / 44100.0);
    let f = 100.0 / 128.0 * 2.0;
    assert!((cl - 2.0 * f * 451.0).abs() < 6.0 && (cr - 2.0 * f * 431.0).abs() < 6.0, "{cl} {cr}");
}

#[test]
fn a_wavetable_slot_can_phase_modulate_the_next_slot() {
    let carrier = [(osc(1, o::TYPE), inv::inv_choice(osc_type::PM, osc_type::COUNT)), (osc(1, o::FINE), fine441().1), (osc(0, o::MIX), 0.0)];
    let level = |send: f32| {
        let mut e = carrier.to_vec();
        e.extend([fine441(), (osc(0, o::PM_OUT), inv::inv_pm_out(send))]);
        let (x, _) = play(&e, 69, 44100.0, 44100);
        (dft(&x, 4000, 39000, 441.0, 44100.0), dft(&x, 4000, 39000, 1323.0, 44100.0))
    };
    let (h1, h3) = level(0.0);
    assert!(h3 / h1 < 0.01, "unmodulated carrier: {}", h3 / h1);
    let (h1, h3) = level(3.0);
    assert!(h3 / h1 > 0.02, "sidebands: {}", h3 / h1);
}

#[test]
fn a_restarted_voice_counts_its_check_interval_from_the_new_note() {
    with_ins(&patch(&wt(&moving())), 44100.0, |ins| {
        render(ins, 44100.0, &[on(0, 69)], 100, 100);
        ins.all_notes_off();
        render_more(ins, &[], 100, 220_000); // the release runs out
        render_more(ins, &[on(220_100, 69)], 220_100, 100);
        let first = ins.voices()[0].wt_wave(0).unwrap();
        render_more(ins, &[], 220_200, 1700);
        assert_eq!(ins.voices()[0].wt_wave(0).unwrap(), first, "no rebuild within 2048 frames of the note");
        render_more(ins, &[], 221_900, 400);
        assert_ne!(ins.voices()[0].wt_wave(0).unwrap(), first);
    });
}

#[test]
fn the_two_channels_start_a_little_apart() {
    let (l, r) = play(&[], 69, 44100.0, 400);
    assert_ne!(l, r);
}

#[test]
fn drive_clips_the_table_towards_a_square() {
    let clean = play(&[fine441()], 69, 44100.0, 44100).0;
    let driven = play(&[fine441(), (sh(sp::WT_DRIVE), 1.0)], 69, 44100.0, 44100).0;
    let h = |x: &[f32], k: u32| dft(x, 4000, 39000, 882.0 * k as f64, 44100.0);
    assert!(h(&clean, 3) / h(&clean, 1) < 0.05);
    assert!((h(&driven, 3) / h(&driven, 1) - 1.0 / 3.0).abs() < 0.06, "{}", h(&driven, 3) / h(&driven, 1));
    assert!(peak(&driven[4000..]) > peak(&clean[4000..]) * 0.99);
    // drive = 1 + 32p: at p = 0.3 the table (amplitude ≈ 0.17: the phase stage's table-range quirk) is scaled by 10.6 and
    // clipped; the third harmonic then follows from the table itself
    let e = [fine441(), (sh(sp::WT_DRIVE), 0.3)];
    let mid = play(&e, 69, 44100.0, 44100).0;
    let table = with_ins(&patch(&wt(&e)), 44100.0, |ins| {
        render(ins, 44100.0, &[on(0, 69)], 100, 100);
        ins.voices()[0].wt_wave(0).unwrap()
    });
    let bin = |k: usize| {
        let (mut re, mut im) = (0.0f64, 0.0f64);
        for (i, w) in table.iter().enumerate() {
            let y = (*w as f64 * 10.6).max(-1.0).min(1.0);
            let a = 2.0 * std::f64::consts::PI * (k * i) as f64 / 512.0;
            re += y * a.cos();
            im += y * a.sin();
        }
        re.hypot(im)
    };
    let want = bin(6) / bin(2); // the table holds two periods
    assert!(want > 0.1, "the table is really clipped: {want}");
    assert!((h(&mid, 3) / h(&mid, 1) - want).abs() < 0.03, "{} vs {want}", h(&mid, 3) / h(&mid, 1));
}

/// A route from LFO 1 (1 Hz, depth 1) as an Offset on the harmonics value of slot 1.
fn moving() -> Vec<(usize, f32)> {
    let t = target::osc(0, target::OSC_SHAPE + sp::WT_HARMONICS as u8);
    merged(&[lfo_1hz(0, true), route(0, source::LFO1, t, false, 1.0), vec![(sh(sp::WT_HARMONICS), 0.3)]])
}

#[test]
fn the_table_is_rebuilt_every_2048_frames_and_only_then() {
    with_ins(&patch(&wt(&moving())), 44100.0, |ins| {
        render(ins, 44100.0, &[on(0, 69)], 100, 100);
        let built = ins.voices()[0].wt_wave(0).unwrap();
        assert!(built.iter().any(|v| *v != 0.0), "built at note-on");
        render_more(ins, &[], 100, 1900); // frame 2000: the LFO has moved the harmonics value, no check yet
        assert_eq!(ins.voices()[0].wt_wave(0).unwrap(), built, "no rebuild before frame 2048");
        render_more(ins, &[], 2000, 100); // frame 2100: the check at 2048 ran
        let second = ins.voices()[0].wt_wave(0).unwrap();
        assert_ne!(second, built, "rebuilt at 2048");
        render_more(ins, &[], 2100, 1900); // frame 4000
        assert_eq!(ins.voices()[0].wt_wave(0).unwrap(), second, "and not in between");
        render_more(ins, &[], 4000, 200); // frame 4200: the check at 4096
        assert_ne!(ins.voices()[0].wt_wave(0).unwrap(), second);
    });
}

#[test]
fn the_phase_stage_drift_starts_from_a_random_value_per_note() {
    // with the mod value at 0 the spectrum passes unchanged; above 0 the drift value scrambles the phases
    let table = |g: f32| {
        with_ins(&patch(&wt(&[(sh(sp::WT_HARMONICS), 0.5), (sh(sp::WT_MOD), g)])), 44100.0, |ins| {
            render(ins, 44100.0, &[on(0, 69)], 100, 100);
            ins.voices()[0].wt_wave(0).unwrap()
        })
    };
    assert_ne!(table(0.0), table(0.6));
}

#[test]
fn an_unchanged_shape_keeps_its_table() {
    with_ins(&patch(&wt(&[])), 44100.0, |ins| {
        render(ins, 44100.0, &[on(0, 69)], 100, 100);
        let built = ins.voices()[0].wt_wave(0).unwrap();
        render_more(ins, &[], 100, 9000);
        assert_eq!(ins.voices()[0].wt_wave(0).unwrap(), built);
    });
}

#[test]
fn rendering_is_independent_of_the_call_split_and_reset_repeats_it() {
    let e = merged(&[wt(&moving()), vec![(sh(sp::WT_LAYERS), 0.5), (sh(sp::WT_LAYER_DETUNE), 0.4), (sh(sp::WT_MOD), 0.6)]]);
    let run = |call: usize| with_ins(&patch(&e), 44100.0, |ins| render(ins, 44100.0, &[on(0, 60), on(1234, 67), off(5000, 60)], 12000, call));
    let whole = run(12000);
    assert!(peak(&whole.0) > 0.01);
    for call in [1, 37, 512] {
        assert_eq!(run(call), whole, "call size {call}");
    }
    with_ins(&patch(&e), 44100.0, |ins| {
        let a = render(ins, 44100.0, &[on(0, 60)], 6000, 600);
        ins.reset();
        let b = render(ins, 44100.0, &[on(0, 60)], 6000, 600);
        assert_eq!(a, b);
    });
}

#[test]
fn the_pool_is_sized_by_the_slot_types() {
    let pm_only = UInstrument::need(&patch(&[])).unwrap();
    let one = UInstrument::need(&patch(&wt(&[]))).unwrap();
    let mut two = wt(&[]);
    two.push((osc(1, o::TYPE), inv::inv_choice(osc_type::WAVETABLE, osc_type::COUNT)));
    let two = UInstrument::need(&patch(&two)).unwrap();
    let voices = 16; // the default capacity
    let table = std::mem::size_of::<crate::wt::Wt>();
    assert!(table > 9000);
    assert!(one >= pm_only + voices * table, "{one} vs {pm_only} + {voices} × {table}");
    assert!(two >= pm_only + 2 * voices * table);
    assert!(two < pm_only + 2 * voices * table + 4096, "no hidden allocation: {two}");
}

#[test]
fn a_patch_without_a_wavetable_slot_allocates_no_tables() {
    let off = UInstrument::need(&patch(&[(osc(0, o::TYPE), inv::inv_choice(osc_type::OFF, osc_type::COUNT))])).unwrap();
    assert_eq!(off, UInstrument::need(&patch(&[])).unwrap());
}
