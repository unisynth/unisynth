//! The read-only voice probe (feature `probe`, change add-device-live-view): what it reports, and that it only reads.

use super::osc_sample::{bank, chunk_of, dls_slot, with_bank};
use super::*;
use crate::env::Stage;
use crate::params::{e, env as penv, f as fp, filt, filter_type as ft, l, lfo};

const RATE: f64 = 44100.0;

#[test]
fn a_held_note_reports_envelope_1_in_sustain_at_its_level() {
    let c = patch(&[(penv(0, e::ATTACK), 0.01), (penv(0, e::DECAY), 0.05), (penv(0, e::SUSTAIN), 0.5)]);
    with_ins(&c, RATE, |ins| {
        render(ins, RATE, &[on(0, 60)], 44100, 512);
        let p = ins.probe();
        assert_eq!(p.voices.len(), 1);
        let v = &p.voices[0];
        assert_eq!((v.note, v.released), (60, false));
        assert_eq!(v.env[0].1, Stage::Sustain);
        assert!((v.env[0].0 - 0.5).abs() < 1e-3, "{}", v.env[0].0);
        assert!((v.age_s - 1.0).abs() < 0.01, "{}", v.age_s);
    });
}

#[test]
fn a_stolen_voice_gets_a_new_id_and_a_released_voice_is_flagged() {
    let c = patch(&[(v::CAPACITY, cap(1)), (v::STEAL, 1.0), (penv(0, e::RELEASE), 0.5)]);
    with_ins(&c, RATE, |ins| {
        render(ins, RATE, &[on(0, 60)], 4410, 512);
        let a = ins.probe().voices[0].id;
        render(ins, RATE, &[on(0, 64)], 4410, 512);
        let b = &ins.probe().voices[0];
        assert_ne!(a, b.id);
        assert_eq!(b.note, 64);
        render(ins, RATE, &[off(0, 64)], 512, 512);
        assert!(ins.probe().voices[0].released);
    });
}

#[test]
fn lfo_phase_advances_by_rate_times_time() {
    let c = patch(&[(lfo(0, l::RATE), 0.5)]);
    let hz = crate::law::lfo_rate(0.5) as f64;
    with_ins(&c, RATE, |ins| {
        render(ins, RATE, &[on(0, 60)], 4096, 512);
        let a = ins.probe().voices[0].lfo[0];
        render(ins, RATE, &[], 8192, 512);
        let b = ins.probe().voices[0].lfo[0];
        let want = (a as f64 + hz * 8192.0 / RATE).fract();
        assert!((b as f64 - want).abs() < 1e-3, "{a} {b} {want}");
    });
}

#[test]
fn sampler_position_advances_through_the_sample() {
    let b = bank(44100, None);
    let c = chunk_of(&dls_slot(&[]), &[]);
    with_bank(&c, Some(&b), RATE, |ins| {
        render(ins, RATE, &[on(0, 60)], 4410, 512);
        let a = ins.probe().voices[0].smp[0].expect("slot 1 is a sampler");
        render(ins, RATE, &[], 4410, 512);
        let z = ins.probe().voices[0].smp[0].unwrap();
        assert!(a > 0.05 && a < 0.15 && z > a + 0.05, "{a} {z}");
        assert_eq!(ins.probe().voices[0].smp[1], None);
    });
}

#[test]
fn modulated_cutoff_is_reported_and_taps_show_the_filter_at_work() {
    let c = patch(&[(filt(0, fp::TYPE), ft::encode(ft::SVF_LP)), (filt(0, fp::CUTOFF), 0.05)]);
    with_ins(&c, RATE, |ins| {
        ins.set_taps(true);
        render(ins, RATE, &[on(0, 72)], 8192, 512);
        let p = ins.probe();
        let hz = crate::probe::cutoff_hz(p.voices[0].cutoff[0].expect("filter 1 is on"));
        assert!((hz - crate::law::cutoff(0.05)).abs() < 1.0, "{hz} vs {}", crate::law::cutoff(0.05));
        assert_eq!(p.voices[0].cutoff[1], None);
        let t = p.taps[0].as_ref().expect("filter 1 tapped");
        let e = |x: &[f32]| x.iter().map(|s| s * s).sum::<f32>();
        assert!(e(&t.0) > 0.0 && e(&t.1) < 0.5 * e(&t.0), "a low-pass far below the note removes energy: {} {} at {hz} Hz", e(&t.0), e(&t.1));
        assert!(p.taps[1].is_none());
    });
}

#[test]
fn taps_do_not_change_the_output() {
    let c = patch(&[(filt(0, fp::TYPE), ft::encode(ft::SVF_LP)), (filt(0, fp::CUTOFF), 0.4), (filt(1, fp::TYPE), ft::encode(ft::SVF_HP))]);
    let evs = [on(0, 60), on(300, 67), off(20000, 60)];
    let a = with_ins(&c, RATE, |ins| render(ins, RATE, &evs, 30000, 333));
    let b = with_ins(&c, RATE, |ins| {
        ins.set_taps(true);
        render(ins, RATE, &evs, 30000, 333)
    });
    assert!(a.0 == b.0 && a.1 == b.1);
}
