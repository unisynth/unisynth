use super::*;
use crate::law::{self, host as inv};

fn near(a: f32, b: f32, tol: f32) -> bool {
    (a - b).abs() <= tol
}

fn bytes() -> impl Iterator<Item = f32> {
    (0..=255u32).map(|i| i as f32 / 255.0)
}

// ---- layout ----

#[test]
fn groups_are_contiguous_and_fill_the_patch() {
    // the frozen v3 layout (design "v3 layout (frozen, task 6.4)")
    assert_eq!([VS, OSC, NOISE, FILT, ENV, LFO, MOD, CTL, DRIVE, QUIRK, STRIP, MOD2, CC, SPEECH, COUNT],
               [16, 23, 98, 102, 126, 162, 178, 218, 221, 225, 230, 261, 285, 291, 294]);
    assert_eq!((vs::AMP_SOURCE, vs::BALANCE), (16, 22));
    assert_eq!((o::LEN, f::LEN, e::LEN, l::LEN, m::LEN), (25, 6, 9, 8, 4));
    assert_eq!(osc(2, o::LEN - 1) + 1, NOISE);
    assert_eq!(filt(3, f::LEN - 1) + 1, ENV);
    assert_eq!(env(3, e::LEN - 1) + 1, LFO);
    assert_eq!(lfo(1, l::LEN - 1) + 1, MOD);
    assert_eq!(modslot(9, m::LEN - 1) + 1, CTL);
    assert_eq!(modslot(10, 0), MOD2);
    assert_eq!(modslot(15, m::LEN - 1) + 1, CC);
    assert_eq!(STRIP + ch::LEN, MOD2);
    assert_eq!(ch::COMP + ch::COMP_LEN, MOD2);
    assert_eq!(cc(7) + 1, SPEECH);
    assert_eq!(speech::PITCH + 1, COUNT);
    assert_eq!(o::SHAPE + o::SHAPES, o::SAMPLE);
    // the one-byte range ends inside the strip, after its ten most-set fields
    assert_eq!(ch::FXROUTE + 1, crate::chunk::SHORT);
    assert!(COUNT <= crate::chunk::MAX_INDEX + 1);
}

#[test]
fn names_are_unique_and_readable() {
    let names: std::vec::Vec<_> = (0..COUNT).map(name).collect();
    let mut sorted = names.clone();
    sorted.sort();
    sorted.dedup();
    assert_eq!(sorted.len(), COUNT);
    assert_eq!(names[v::MASTER], "master");
    assert_eq!(names[osc(1, o::GAIN)], "osc2.gain");
    assert_eq!(names[filt(0, f::CUTOFF)], "filt1.cutoff");
    assert_eq!(names[env(3, e::RELEASE)], "env4.release");
    assert_eq!(names[lfo(1, l::RATE)], "lfo2.rate");
    assert_eq!(names[modslot(6, m::AMOUNT)], "mod7.amount");
    assert_eq!(names[NOISE + n::LP], "noise.lp");
    assert_eq!(names[modslot(13, m::TARGET)], "mod14.target");
    assert_eq!(names[cc(7)], "ctl.cc7");
    assert_eq!(names[lfo(1, l::POLARITY)], "lfo2.polarity");
    assert_eq!(names[quirk::INTEGER_FM], "quirk.integer_fm");
    assert_eq!(names[ch::DC_FILTER], "strip.dc_filter");
    assert_eq!(names[osc(2, o::COMBINE)], "osc3.combine");
    assert_eq!(names[ch::COMP + 4], "strip.comp_threshold");
    assert!(names.iter().all(|n| !n.starts_with("param")), "every index has a real name");
}

#[test]
fn every_index_has_a_law_of_the_right_kind() {
    use Law::*;
    assert_eq!(law_of(v::MASTER), Gain);
    assert_eq!(law_of(v::CAPACITY), Count(1, 64));
    assert_eq!(law_of(v::UNISON), Count(1, 16));
    assert_eq!(law_of(osc(2, o::TYPE)), Choice(13));
    assert_eq!(law_of(osc(0, o::SHAPE + 4)), Shape);
    assert_eq!(law_of(osc(1, o::FEEDBACK)), Feedback);
    assert_eq!(law_of(NOISE + n::HP), Cutoff);
    assert_eq!(law_of(filt(2, f::CUTOFF)), Cutoff);
    assert_eq!(law_of(filt(2, f::TYPE)), Choice(11));
    assert_eq!(law_of(filt(2, f::FORM)), Choice(2));
    assert_eq!(law_of(env(1, e::LAW)), Choice(3));
    assert_eq!(law_of(env(1, e::CURVE)), Choice(4));
    assert_eq!(law_of(env(0, e::ATTACK)), EnvTime);
    assert_eq!(law_of(env(0, e::SUSTAIN)), Unit);
    assert_eq!(law_of(lfo(0, l::RATE)), LfoRate);
    assert_eq!(law_of(modslot(0, m::TARGET)), Index);
    assert_eq!(law_of(modslot(0, m::AMOUNT)), ModAmount);
    assert_eq!(law_of(modslot(11, m::SOURCE)), Choice(17));
    assert_eq!(law_of(modslot(15, m::OPERATION)), Choice(3));
    assert_eq!(law_of(vs::KEYSYNC), Choice(3));
    assert_eq!(law_of(lfo(0, l::SYNC)), Choice(3));
    assert_eq!(law_of(quirk::EXACT_MATH), Choice(2));
    assert_eq!(law_of(ch::COMP + 4), Steps);
    assert_eq!(law_of(cc(3)), Steps);
    assert_eq!(law_of(osc(0, o::SAMPLE)), Raw);
    assert_eq!(law_of(COUNT + 5), Unit);
}

// ---- defaults ----

#[test]
fn defaults_are_neutral_and_audible() {
    let d = DEFAULTS;
    assert!(d.iter().all(|x| (0.0..=1.0).contains(x)));
    assert_eq!(law::gain(d[v::MASTER]), 1.0);
    assert_eq!(law::pan(d[v::PAN]), law::pan(0.5));
    assert_eq!(law::count(d[v::CAPACITY], 1, 64), 16);
    assert_eq!(law::count(d[v::UNISON], 1, 16), 1);
    assert_eq!(law::choice(d[v::STEAL], 2), 1, "steal oldest");
    assert_eq!(law::choice(d[v::MODE], 2), 0, "poly");
    assert_eq!(law::transpose(d[v::TRANSPOSE]), 0.0);
    assert_eq!(law::gain(d[v::NOISE_LEVEL]), 0.0);
    for i in 0..OSC_SLOTS {
        assert_eq!(law::choice(d[osc(i, o::TYPE)], osc_type::COUNT), osc_type::OFF);
        assert_eq!(law::coarse(d[osc(i, o::COARSE)]), 0.0);
        assert_eq!(law::fine(d[osc(i, o::FINE)]), 0.0);
        assert_eq!(law::octave(d[osc(i, o::OCTAVE)]), 0.0);
        assert_eq!(law::ratio(d[osc(i, o::RATIO)]), 1.0);
        assert_eq!(law::ratio_fine(d[osc(i, o::RATIO_FINE)]), 0.0);
        assert_eq!(law::gain(d[osc(i, o::GAIN)]), 1.0);
        assert_eq!(d[osc(i, o::MIX)], 1.0);
        assert_eq!(d[osc(i, o::PM_OUT)], 0.0);
    }
    for i in 0..FILT_SLOTS {
        assert_eq!(law::choice(d[filt(i, f::TYPE)], filter_type::COUNT), filter_type::OFF);
        assert_eq!(law::cutoff(d[filt(i, f::CUTOFF)]), 20_000.0);
    }
    for i in 0..ENV_SLOTS {
        assert_eq!(d[env(i, e::SUSTAIN)], 1.0);
        assert_eq!(law::env_ms(d[env(i, e::ATTACK)]), 1.0, "instant attack");
    }
    for i in 0..MOD_SLOTS {
        assert_eq!(crate::modmx::source::decode(d[modslot(i, m::SOURCE)]), 0, "no source");
    }
    assert_eq!(law::cutoff(d[NOISE + n::HP]), 20.0);
    assert_eq!(law::cutoff(d[NOISE + n::LP]), 20_000.0);
    // the settings are off, no quirk is set, the 7-bit parts hold the source's initial patch values
    for i in [vs::AMP_SOURCE, vs::PAN_STAGE, vs::DC_FILTER, vs::LAW, vs::KEYSYNC, vs::ROUTING, ch::ON, ch::DC_FILTER, speech::ON] {
        assert_eq!(d[i], 0.0, "{}", name(i));
    }
    assert!((QUIRK..QUIRK + quirk::LEN).all(|q| !quirk_on(&d, q)));
    for k in 0..LFO_SLOTS {
        assert_eq!(law::choice(d[lfo(k, l::SYNC)], lfo_sync::COUNT), lfo_sync::RESTART);
    }
    assert_eq!(v2_raw(d[cc(7)]), 127.0);
    assert_eq!(v2_raw(d[ch::COMP + 4]), 90.0);
}

#[test]
fn choices_decode_every_byte_and_their_own_values() {
    for (n, dec, enc) in [
        (osc_type::COUNT, osc_type::decode as fn(f32) -> usize, osc_type::encode as fn(usize) -> f32),
        (filter_type::COUNT, filter_type::decode, filter_type::encode),
        (crate::modmx::source::COUNT, crate::modmx::source::decode, crate::modmx::source::encode),
        (crate::modmx::op::COUNT, crate::modmx::op::decode, crate::modmx::op::encode),
    ] {
        for k in 0..n {
            assert_eq!(dec(enc(k)), k);
            assert_eq!(enc(k).to_bits(), inv::inv_choice(k, n).to_bits());
        }
        assert!(bytes().all(|p| dec(p) < n));
    }
}

// ---- law spot values ----

#[test]
fn law_spot_values() {
    assert_eq!(law::cutoff(0.0), 20.0);
    assert_eq!(law::cutoff(1.0), 20_000.0);
    assert!(near(law::cutoff(0.5), 20.0 + 19_980.0 / 4.0, 1e-2));
    assert_eq!(law::gain(0.25), 1.0);
    assert_eq!(law::gain(1.0), 16.0);
    assert_eq!(law::time(0.0), 0.0);
    assert_eq!(law::time(1.0), 10.0);
    assert!(near(law::time(0.5), 0.625, 1e-6));
    assert!(near(law::lfo_rate(0.0), 0.01, 1e-7));
    assert!(near(law::lfo_rate(1.0), 100.0, 1e-3));
    assert!(near(law::lfo_rate(0.5), 1.0, 1e-4));
    assert_eq!((law::coarse(0.0), law::coarse(0.5), law::coarse(1.0)), (-24.0, 0.0, 24.0));
    assert_eq!((law::octave(0.0), law::octave(0.5), law::octave(1.0)), (-4.0, 0.0, 4.0));
    assert_eq!((law::fine(0.0), law::fine(0.5), law::fine(1.0)), (-1.0, 0.0, 1.0));
    assert_eq!((law::ratio(0.0), law::ratio(1.0)), (1.0, 33.0));
    assert_eq!(law::ratio_fine(1.0), 1.0);
    assert_eq!(law::fixed_hz(1.0), 1000.0);
    assert_eq!(law::env_ms(0.0), 1.0);
    assert_eq!(law::env_ms(1.0), 5001.0);
    let (l, r) = law::pan(0.5);
    assert!(near(l, r, 1e-7) && near(l * l + r * r, 1.0, 1e-6));
    assert_eq!(law::pan(0.0), (1.0, 0.0));
    assert_eq!(law::choice(0.0, 5), 0);
    assert_eq!(law::choice(1.0, 5), 4);
    assert_eq!(law::choice(0.5, 5), 2);
    assert_eq!((law::count(0.0, 1, 64), law::count(1.0, 1, 64)), (1, 64));
    assert_eq!(law::index(1.0), 255);
    assert_eq!(law::scale_amount(1.0), 10.0);
    assert_eq!(law::scale_amount(0.5), 1.0);
    assert_eq!(law::offset_amount(1.0, 36.0), 36.0);
    assert_eq!(law::offset_amount(0.0, 36.0), -36.0);
    assert_eq!(law::offset_amount(0.5, 36.0).abs() < 0.01, true);
}

#[test]
fn laws_are_total_on_garbage() {
    for p in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, -3.0, 7.5, 1e30] {
        let vals = [
            law::cutoff(p), law::gain(p), law::lfo_rate(p), law::time(p), law::transpose(p), law::coarse(p),
            law::fine(p), law::octave(p), law::ratio(p), law::ratio_fine(p), law::fixed_hz(p), law::feedback(p),
            law::pm_out(p), law::env_ms(p), law::scale_amount(p), law::offset_amount(p, 36.0),
            law::pan(p).0, law::pan(p).1,
        ];
        assert!(vals.iter().all(|v| v.is_finite()), "p = {p}");
        assert!(law::choice(p, 5) < 5 && (1..=64).contains(&law::count(p, 1, 64)));
        let _ = law::index(p);
    }
    assert_eq!(law::choice(0.3, 1), 0);
    assert_eq!(law::choice(0.3, 0), 0);
}

// ---- inverses ----

#[test]
fn continuous_laws_round_trip_over_every_byte() {
    type F = (&'static str, fn(f32) -> f32, fn(f32) -> f32);
    let laws: [F; 12] = [
        ("cutoff", law::cutoff, inv::inv_cutoff),
        ("gain", law::gain, inv::inv_gain),
        ("lfo_rate", law::lfo_rate, inv::inv_lfo_rate),
        ("time", law::time, inv::inv_time),
        ("transpose", law::transpose, inv::inv_transpose),
        ("fine", law::fine, inv::inv_fine),
        ("fixed_hz", law::fixed_hz, inv::inv_fixed_hz),
        ("feedback", law::feedback, inv::inv_feedback),
        ("pm_out", law::pm_out, inv::inv_pm_out),
        ("env_ms", law::env_ms, inv::inv_env_ms),
        ("scale_amount", law::scale_amount, inv::inv_scale_amount),
        ("fine again", law::fine, inv::inv_fine),
    ];
    for (name, map, back) in laws {
        for p in bytes() {
            let q = back(map(p));
            assert!(near(p, q, 2e-4), "{name}: {p} -> {} -> {q}", map(p));
        }
    }
    for p in bytes() {
        let q = inv::inv_offset_amount(law::offset_amount(p, 36.0), 36.0);
        assert!(near(p, q, 2e-4), "offset_amount {p} -> {q}");
    }
}

#[test]
fn integer_laws_round_trip_exactly() {
    for i in 0..9 {
        assert_eq!(law::choice(inv::inv_choice(i, 9), 9), i);
    }
    for v in 1..=64 {
        assert_eq!(law::count(inv::inv_count(v, 1, 64), 1, 64), v);
    }
    for v in 1..=16 {
        assert_eq!(law::count(inv::inv_count(v, 1, 16), 1, 16), v);
    }
    for i in 0..=255u8 {
        assert_eq!(law::index(inv::inv_index(i)), i);
    }
    for c in -24..=24 {
        assert_eq!(law::coarse(inv::inv_coarse(c as f32)), c as f32);
    }
    for o in -4..=4 {
        assert_eq!(law::octave(inv::inv_octave(o as f32)), o as f32);
    }
    for r in 1..=33 {
        assert_eq!(law::ratio(inv::inv_ratio(r as f32)), r as f32);
    }
    // Law::map / inverse agree with the free functions
    assert_eq!(Law::Choice(5).inverse(3.0), Some(0.75));
    assert_eq!(Law::Cutoff.inverse(20_000.0), Some(1.0));
    assert_eq!(Law::Shape.inverse(1.0), None);
    assert_eq!(Law::Gain.map(0.25), 1.0);
}

// ---- coverage of the original ranges (design D2) ----

/// A original physical value is covered when the law reaches it within `tol` (relative).
fn covered(law: Law, v: f32, tol: f32) -> bool {
    let p = law.inverse(v).expect("invertible");
    (0.0..=1.0).contains(&p) && near(law.map(p), v, v.abs() * tol + 1e-6)
}

#[test]
fn cutoff_covers_every_original_filter_range() {
    // WaveSabre 20 + 19 980p²; Tunefish ladder/notch 30 + 20 000p², RBJ 30 + 10 000p² (scale-compatible: re-mapped)
    for hz in [20.0, 30.0, 440.0, 10_030.0, 20_000.0] {
        assert!(covered(Law::Cutoff, hz, 1e-4), "{hz}");
    }
    // ladder/notch top 20 030 Hz is 0.15 % over the law's 20 000 Hz: clamped
    assert!(near(law::cutoff(inv::inv_cutoff(20_030.0)), 20_030.0, 31.0));
    // a Tunefish p·M (M up to 10) saturates the law like it saturates the original one
    assert_eq!(law::cutoff(1.5), 20_000.0);
}

#[test]
fn lfo_rate_covers_vibrato_and_tunefish_lfo() {
    for hz in [7.0, 0.1 * 70.0, 77.0, 0.05, 1.0, 7.96] {
        assert!(covered(Law::LfoRate, hz, 2e-3), "{hz}");
    }
    // WaveSabre vibrato law (p² + 0.1)·70 over the whole parameter range
    for p in bytes() {
        let hz = (p * p + 0.1) * 70.0;
        assert!(near(law::lfo_rate(inv::inv_lfo_rate(hz)), hz, hz * 2e-3));
    }
    // Tunefish LFO 50p² rad/s over 0..1 (p = 0 is 0 Hz: the law's floor of 0.01 Hz is a 1.6 rad cycle per 100 s)
    for p in (1..=100).map(|i| i as f32 / 100.0) {
        let hz = 50.0 * p * p / (2.0 * core::f32::consts::PI);
        if hz >= 0.01 {
            assert!(covered(Law::LfoRate, hz, 2e-3), "{hz}");
        }
    }
}

#[test]
fn pitch_laws_cover_original_pitch_controls() {
    for rise in [0.0, 6.0, 24.0] {
        assert!(near(law::transpose(inv::inv_transpose(rise)), rise, 1e-4)); // rise·24 semitones
    }
    for c in 0..=24 {
        assert_eq!(law::coarse(inv::inv_coarse(c as f32)), c as f32); // Slaughter floor(d·24.99)
    }
    for c in -12..=11 {
        assert_eq!(law::coarse(inv::inv_coarse(c as f32)), c as f32); // Specimen floor((2c−1)·12)
    }
    for f in [0.0, 0.5, 1.0, -1.0] {
        assert!(near(law::fine(inv::inv_fine(f)), f, 1e-6));
    }
    for oct in -4..=4 {
        assert_eq!(law::octave(inv::inv_octave(oct as f32)), oct as f32); // Tunefish 2^(4 − round(8p))
    }
    for r in 1..=33 {
        assert_eq!(law::ratio(inv::inv_ratio(r as f32)), r as f32); // Falcon 1 + floor(33p)
    }
}

#[test]
fn level_and_modulation_laws_reach_the_original_extremes() {
    assert!(law::gain(1.0) >= 13.25, "Falcon operator 1 factor");
    assert!(law::gain(1.0) >= 11.0, "Tunefish gain curve maximum");
    assert!(law::gain(1.0) >= 0.4 * 11.0, "Tunefish voice volume × output gain");
    assert!(law::feedback(1.0) >= 13.25 * 0.5, "Falcon operator 2 feedback fb²/2·13.25");
    assert!(law::pm_out(1.0) >= 1.0, "Falcon feed-forward ff²");
    assert!(law::pm_out(inv::inv_pm_out(0.5)) > 0.49);
    assert_eq!(law::time(inv::inv_time(10.0)), 10.0, "WaveSabre slide 10·p⁴");
    assert!(near(law::env_ms(inv::inv_env_ms(5001.0)), 5001.0, 1e-2), "WaveSabre envelope p²·5000 + 1");
    assert_eq!(law::fixed_hz(inv::inv_fixed_hz(1000.0)), 1000.0);
    assert!(near(law::scale_amount(inv::inv_scale_amount(10.0)), 10.0, 1e-4), "Tunefish mod amount");
    assert!(near(law::offset_amount(inv::inv_offset_amount(36.0, 36.0), 36.0), 36.0, 1e-3), "pitch env ±36");
    assert!(near(law::offset_amount(inv::inv_offset_amount(-19_980.0, 19_980.0), 19_980.0), -19_980.0, 1.0));
}

// ---- resolution (design D2: "no coarser than the original law" where shared, perceptual bounds elsewhere) ----

#[test]
fn cutoff_resolution_is_no_coarser_than_the_original_laws() {
    let step = 1.0 / 255.0;
    // WaveSabre: identical law and quantization; Tunefish `.tfm` values are byte/100, RBJ 30 + 10 000p²
    let mut hz = 60.0f32;
    while hz < 15_000.0 {
        let p = inv::inv_cutoff(hz);
        let unified = (law::cutoff(p + step) - law::cutoff(p)) / hz;
        let ws = unified; // same law, same step
        let pl = ((hz - 30.0) / 10_000.0).sqrt();
        let tf_rbj = ((30.0 + 10_000.0 * (pl + 0.01).powi(2)) - hz) / hz;
        let pl2 = ((hz - 30.0) / 20_000.0).sqrt();
        let tf_ladder = ((30.0 + 20_000.0 * (pl2 + 0.01).powi(2)) - hz) / hz;
        assert!(unified <= ws + 1e-9);
        assert!(unified <= tf_rbj.min(tf_ladder) + 1e-6, "{hz} Hz: {unified} vs {tf_rbj} / {tf_ladder}");
        hz *= 1.2;
    }
}

#[test]
fn perceptual_resolution_bounds() {
    let step = 1.0 / 255.0;
    // LFO rate: ≤ 4 % per byte everywhere (rate JND ≈ 5 %)
    for p in bytes().filter(|p| *p < 1.0) {
        let (a, b) = (law::lfo_rate(p), law::lfo_rate(p + step));
        assert!((b - a) / a <= 0.04, "lfo {p}");
    }
    // transpose (rise): ≤ 0.2 semitones per byte
    assert!(48.0 / 255.0 <= 0.2);
    // gain: ≤ 0.7 dB per byte down to −40 dB under the maximum (p ≥ 0.1)
    for p in bytes().filter(|p| (0.1..1.0).contains(p)) {
        let db = 20.0 * (law::gain(p + step) / law::gain(p)).log10();
        assert!(db <= 0.7, "gain {p}: {db} dB");
    }
    // pitch offset amounts: ≤ 0.1 semitone (10 cents) per byte up to ±1 (vibrato), ≤ 0.35 semitones per byte up to ±12, ≤ 0.6 up to ±36
    for p in bytes().filter(|p| *p < 1.0) {
        let (a, b) = (law::offset_amount(p, 36.0), law::offset_amount(p + step, 36.0));
        let d = (b - a).abs();
        if a.abs() <= 1.0 && b.abs() <= 1.0 {
            assert!(d <= 0.1, "vibrato range {p}: {d}");
        }
        if a.abs() <= 12.0 && b.abs() <= 12.0 {
            assert!(d <= 0.35, "{p}: {d}");
        }
        assert!(d <= 0.6, "{p}: {d}");
    }
    // envelope time, slide time, cutoff: the WaveSabre laws themselves, so exactly as fine as the original byte
    for p in bytes() {
        assert_eq!(law::env_ms(p), p * p * 5000.0 + 1.0);
        assert!(near(law::time(p), 10.0 * p.powi(4), 1e-6));
        assert!(near(law::cutoff(p), 20.0 + (20_000.0 - 20.0) * p * p, 1e-3));
    }
}

#[test]
fn only_the_sample_indices_are_raw_and_every_shape_value_is_normalized() {
    for i in 0..COUNT {
        assert_eq!(is_raw(i), i >= OSC && i < NOISE && (i - OSC) % o::LEN == o::SAMPLE, "{i}");
        if is_raw(i) {
            assert!(is_structural(i));
            assert!(matches!(host::law_of(i), host::Law::Raw), "{i}");
        }
    }
    for slot in 0..OSC_SLOTS {
        let last = osc(slot, o::SHAPE + shape::SMP_SOURCE + 1);
        assert_eq!(last, osc(slot, o::SHAPE + o::SHAPES - 1), "shape[8] is the last shape value of the slot");
        assert!(!is_raw(last) && !is_structural(last), "shape[8] is an ordinary normalized value");
        assert!(matches!(host::law_of(last), host::Law::Shape));
        assert_eq!(name(smp_index(slot)), format!("osc{}.sample", slot + 1));
        assert_eq!(DEFAULTS[smp_index(slot)], 0.0);
    }
}

#[test]
fn controllers_have_rest_defaults_names_and_laws() {
    assert_eq!((name(ctl::WHEEL), name(ctl::BEND), name(ctl::BEND_RANGE)), ("ctl.wheel".into(), "ctl.bend".into(), "ctl.bend_range".into()));
    assert_eq!((DEFAULTS[ctl::WHEEL], DEFAULTS[ctl::BEND]), (0.0, 0.5));
    assert!(near(law::bend_range(DEFAULTS[ctl::BEND_RANGE]), 2.0, 1e-5));
    assert!(near(law::bend_range(1.0), 24.0, 1e-5) && law::bend_range(0.0) == 0.0);
    assert!(near(law::bend_semitones(1.0, inv::inv_bend_range(2.0)), 2.0, 1e-5));
    assert!(near(law::bend_semitones(0.0, inv::inv_bend_range(7.0)), -7.0, 1e-5));
    assert_eq!(law::bend_semitones(0.5, 1.0), 0.0);
    assert!(matches!(host::law_of(ctl::BEND_RANGE), host::Law::BendRange));
    assert!(matches!(host::law_of(ctl::WHEEL), host::Law::Unit) && matches!(host::law_of(ctl::BEND), host::Law::Unit));
    assert!(!is_raw(ctl::WHEEL) && !is_structural(ctl::BEND_RANGE), "controllers are automatable");
    for st in [0.0f32, 2.0, 12.0, 24.0] {
        assert!(near(host::Law::BendRange.map(host::Law::BendRange.inverse(st).unwrap()), st, 1e-4));
    }
}

/// change unify-v2-algorithms 1.2: `rename::OLD` covers every parameter of the old layout, by old index; its new names
/// are unique, free of source-synth names, and (6.5) every non-empty one is a v3 name.
#[test]
fn the_rename_map_covers_every_old_parameter() {
    use super::rename::OLD;
    assert_eq!(OLD.len(), old::COUNT);
    let mut seen = std::collections::BTreeSet::new();
    for (i, (_, new)) in OLD.iter().enumerate() {
        if new.is_empty() {
            continue;
        }
        assert_eq!(old::name(i), *new, "old index {i}");
        assert!(seen.insert(*new), "{new} twice");
        assert!(!has_source_name(new), "{new} names a source synth");
        assert!((0..COUNT).any(|j| name(j) == *new), "{new}: no v3 index");
    }
    for i in 0..COUNT {
        assert!(!has_source_name(&name(i)), "{} names a source synth", name(i));
    }
}

/// A source-synth name (change unify-v2-algorithms, "Functional names"): substrings, or the short forms as a token.
pub(crate) fn has_source_name(s: &str) -> bool {
    // words, not substrings: `env2` is envelope 2, not V2
    let l = s.to_ascii_lowercase();
    ["tunefish", "wavesabre", "ronan", "moog", "slaughter", "falcon", "cauldron"].iter().any(|x| l.contains(x))
        || l.split(|c: char| !c.is_ascii_alphanumeric()).any(|t| t == "tf" || t == "ws" || t.starts_with("v2"))
}

/// The player has no format version (change unify-v2-algorithms 6.8): `era` is read only by the old layout and the
/// migration (host-only).
#[test]
fn the_player_reads_no_format_version() {
    fn walk(dir: &std::path::Path, out: &mut std::vec::Vec<(std::string::String, std::string::String)>) {
        for e in std::fs::read_dir(dir).unwrap() {
            let p = e.unwrap().path();
            if p.is_dir() {
                walk(&p, out);
            } else if p.extension().is_some_and(|x| x == "rs") {
                out.push((p.display().to_string(), std::fs::read_to_string(&p).unwrap()));
            }
        }
    }
    let mut files = std::vec::Vec::new();
    walk(&std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src"), &mut files);
    let mut bad = std::vec::Vec::new();
    for (path, text) in &files {
        if path.contains("params/old") || path.contains("migrate") || path.contains("/tests") || path.ends_with("tests.rs") {
            continue;
        }
        for (n, line) in text.lines().enumerate() {
            let code = line.split("//").next().unwrap_or("");
            if code.contains("era(") || code.contains("era_settings") || code.contains("v2::ERA") {
                bad.push(std::format!("{path}:{}: {}", n + 1, line.trim()));
            }
        }
    }
    assert!(bad.is_empty(), "format-version reads in the player:\n{}", bad.join("\n"));
}

