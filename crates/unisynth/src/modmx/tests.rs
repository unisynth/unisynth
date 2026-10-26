use super::*;
use crate::law::host as inv;
use crate::params::{lfo, modslot, DEFAULTS};

fn near(a: f32, b: f32) -> bool {
    (a - b).abs() < 1e-5
}

/// Configures slot `i`. `amount` is the physical amount (a Scale factor or an Offset addend).
fn set(p: &mut [f32; COUNT], i: usize, src: usize, tgt: u8, scale: bool, amount: f32) {
    p[modslot(i, m::SOURCE)] = inv::inv_choice(src, source::COUNT);
    p[modslot(i, m::TARGET)] = inv::inv_index(tgt);
    p[modslot(i, m::OPERATION)] = inv::inv_choice(if scale { 0 } else { 1 }, 2);
    p[modslot(i, m::AMOUNT)] =
        if scale { inv::inv_scale_amount(amount) } else { inv::inv_offset_amount(amount, target::offset_range(tgt)) };
}

fn sources() -> Sources {
    Sources { env: [1.0, 0.5, 0.0, 0.25], lfo_shape: [1.0, 0.0], wheel: 0.75, velocity: 0.5, key: 0.25 }
}

#[test]
fn an_empty_matrix_is_neutral() {
    let mut mm = Matrix::new();
    mm.eval(&DEFAULTS, &sources());
    for t in 0..target::COUNT {
        assert_eq!((mm.scale(t), mm.offset(t)), (1.0, 0.0), "target {t}");
    }
}

#[test]
fn scale_slots_on_one_target_multiply() {
    let mut p = DEFAULTS;
    let cutoff = target::filter_cutoff(0);
    set(&mut p, 0, source::VELOCITY, cutoff, true, 1.0); // 1.0 · 0.5
    set(&mut p, 5, source::VELOCITY, cutoff, true, 1.0); // 1.0 · 0.5
    let mut mm = Matrix::new();
    mm.eval(&p, &sources());
    assert!(near(mm.scale(cutoff), 0.25));
    assert_eq!(mm.offset(cutoff), 0.0);
    assert_eq!(mm.scale(target::filter_cutoff(1)), 1.0, "other targets untouched");
}

#[test]
fn offset_slots_add_in_the_target_unit() {
    let mut p = DEFAULTS;
    set(&mut p, 0, source::ENV1, target::PITCH, false, 12.0); // env 1 is fully open
    set(&mut p, 1, source::ENV1 + 1, target::PITCH, false, 7.0); // env 2 at 0.5 → 3.5
    set(&mut p, 2, source::ENV1, target::PITCH, false, -2.0);
    let mut mm = Matrix::new();
    mm.eval(&p, &sources());
    assert!((mm.offset(target::PITCH) - (12.0 + 3.5 - 2.0)).abs() < 0.02, "{}", mm.offset(target::PITCH));
    assert_eq!(mm.scale(target::PITCH), 1.0);
}

#[test]
fn sources_have_their_values() {
    let mut p = DEFAULTS;
    let t = target::filter_cutoff(2);
    for (src, want) in [(source::WHEEL, 0.75), (source::VELOCITY, 0.5), (source::KEY, 0.25), (source::ENV1 + 3, 0.25)] {
        set(&mut p, 0, src, t, true, 1.0);
        let mut mm = Matrix::new();
        mm.eval(&p, &sources());
        assert!(near(mm.scale(t), want), "source {src}");
    }
    // an inert slot: no source, or no target
    set(&mut p, 0, source::NONE, t, true, 1.0);
    let mut mm = Matrix::new();
    mm.eval(&p, &sources());
    assert_eq!(mm.scale(t), 1.0);
    set(&mut p, 0, source::WHEEL, target::NONE, true, 1.0);
    mm.eval(&p, &sources());
    assert_eq!(mm.scale(t), 1.0);
}

#[test]
fn lfo_sources_see_the_scale_and_offset_views() {
    let mut p = DEFAULTS;
    p[lfo(0, pm::l::DEPTH)] = 0.5; // d = 0.25
    let t = target::filter_cutoff(0);
    set(&mut p, 0, source::LFO1, t, true, 1.0);
    let mut mm = Matrix::new();
    let mut s = sources();
    s.lfo_shape[0] = 0.0;
    mm.eval(&p, &s);
    assert!(near(mm.scale(t), 0.75), "1 − d + d·0");
    set(&mut p, 0, source::LFO1, target::PITCH, false, 12.0);
    mm.eval(&p, &s);
    s.lfo_shape[0] = 1.0;
    let lo = mm.offset(target::PITCH);
    mm.eval(&p, &s);
    let hi = mm.offset(target::PITCH);
    assert!((lo + 3.0).abs() < 0.05 && (hi - 3.0).abs() < 0.05, "±d·12: {lo} {hi}");
}

#[test]
fn a_slot_aimed_at_a_slot_amount_modulates_it_one_pass_later() {
    // slot 1: LFO 1 → cutoff; slot 2: env 2 (0.5) → slot 1's amount
    let mut p = DEFAULTS;
    let cutoff = target::filter_cutoff(0);
    set(&mut p, 0, source::LFO1, cutoff, true, 1.0);
    set(&mut p, 1, source::ENV1 + 1, target::SLOT, true, 1.0);
    let mut mm = Matrix::new();
    mm.eval(&p, &sources());
    assert!(near(mm.scale(cutoff), 1.0), "first pass: the own factor is still 1");
    mm.eval(&p, &sources());
    assert!(near(mm.scale(cutoff), 0.5), "second pass: × env 2");
    // self-modulation: a slot aimed at its own amount feeds on its previous result
    let mut p = DEFAULTS;
    set(&mut p, 0, source::ENV1 + 1, target::SLOT, true, 1.0); // 0.5 · own
    let mut mm = Matrix::new();
    let mut last = vec![];
    for _ in 0..4 {
        mm.eval(&p, &sources());
        last.push(mm.scale(target::SLOT));
    }
    assert_eq!(last, [0.5, 0.25, 0.125, 0.0625].map(|x| x as f32).to_vec());
}

#[test]
fn lfo_depth_is_modulated_by_earlier_slots_this_pass_and_later_ones_the_pass_before() {
    let build = |depth_slot: usize, route_slot: usize| {
        let mut p = DEFAULTS;
        p[lfo(0, pm::l::DEPTH)] = 1.0;
        set(&mut p, depth_slot, source::VELOCITY, target::LFO_DEPTH, true, 1.0); // depth × 0.5 → d = 0.25
        set(&mut p, route_slot, source::LFO1, target::filter_cutoff(0), true, 1.0);
        p
    };
    let mut s = sources();
    s.lfo_shape[0] = 0.0;
    // depth slot first: this pass already sees depth 0.5 → 1 − 0.25 + 0 = 0.75
    let mut mm = Matrix::new();
    mm.eval(&build(0, 1), &s);
    assert!(near(mm.scale(target::filter_cutoff(0)), 0.75));
    // depth slot last: the first pass still reads the full depth (shape 0 → 0), the next pass sees the result
    let mut mm = Matrix::new();
    mm.eval(&build(1, 0), &s);
    assert!(near(mm.scale(target::filter_cutoff(0)), 0.0));
    mm.eval(&build(1, 0), &s);
    assert!(near(mm.scale(target::filter_cutoff(0)), 0.75));
}

#[test]
fn target_table() {
    use target::*;
    assert_eq!((osc(0, OSC_PITCH), osc(0, OSC_SHAPE + 8), osc(1, OSC_PITCH), osc(2, OSC_SHAPE + 8)), (4, 18, 19, 48));
    assert_eq!(filter_cutoff(3), 58);
    assert_eq!(filter_resonance(3), 59);
    assert_eq!(SLOT + 11, COUNT - 1);
    for t in [PITCH, GAIN, NOISE_LEVEL, osc(0, OSC_PITCH), osc(2, OSC_GAIN), SLOT, SLOT + 11] {
        assert!(scales_physical(t), "{}", name(t));
    }
    for t in [PAN, osc(0, OSC_MIX), osc(1, OSC_PAN), osc(1, OSC_SHAPE), NOISE_HP, filter_cutoff(0), filter_resonance(2), ENV_DECAY, LFO_DEPTH] {
        assert!(!scales_physical(t), "{}", name(t));
    }
    assert_eq!((offset_range(PITCH), offset_range(filter_cutoff(0)), offset_range(filter_resonance(0))), (36.0, 19_980.0, 1.0));
    assert_eq!((offset_range(osc(1, OSC_FEEDBACK)), offset_range(osc(2, OSC_PM_OUT)), offset_range(osc(0, OSC_PAN))), (8.0, 4.0, 1.0));
    let names: Vec<_> = (0..COUNT).map(name).collect();
    let mut u = names.clone();
    u.sort();
    u.dedup();
    assert_eq!(u.len(), COUNT as usize, "unique names");
    assert_eq!((names[1].as_str(), names[49].as_str(), names[52].as_str(), names[60].as_str(), names[77].as_str()),
        ("pitch", "noise.level", "filt1.cutoff", "env1.decay", "mod12.amount"));
}

#[test]
fn the_table_gives_every_target_what_scanning_the_slots_gives() {
    let mut p = DEFAULTS;
    let cutoff = target::filter_cutoff(0);
    set(&mut p, 0, source::VELOCITY, cutoff, true, 1.0);
    set(&mut p, 1, source::ENV1, cutoff, true, 0.7);
    set(&mut p, 2, source::WHEEL, cutoff, false, 800.0);
    set(&mut p, 3, source::LFO1, cutoff, false, -300.0);
    set(&mut p, 4, source::KEY, target::GAIN, true, 0.9);
    set(&mut p, 5, source::ENV1 + 1, target::PITCH, false, 2.5);
    set(&mut p, 9, source::VELOCITY, target::SLOT + 2, true, 0.6);
    let mut mm = Matrix::new();
    mm.eval(&p, &sources());
    let tab = mm.tab();
    for t in 0..target::COUNT {
        assert_eq!(tab.scale(t).to_bits(), mm.scale(t).to_bits(), "scale of target {t}");
        assert_eq!(tab.offset(t).to_bits(), mm.offset(t).to_bits(), "offset of target {t}");
    }
    assert!(mm.scale(cutoff) != 1.0 && mm.offset(cutoff) != 0.0, "the case has something to compare");
    // a target past the table asks for nothing
    assert_eq!((tab.scale(200), tab.offset(200)), (1.0, 0.0));
}
