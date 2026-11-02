//! V2 modules in the voice (change add-v2-import): the spec scenarios of `unified-synth` for the V2 oscillator,
//! filters and routing, voice distortion, envelope and LFO modes, and the channel stage.

use super::*;
use crate::modmx::{op, source, target};
use crate::params::old::{self as pm, ch, e, env_mode, f, filter_type as ft, l, lx, shape, v, v2};

/// A V2 patch: the V2 voice, envelope 1 in V2 mode at full level, `oscs` as `(mode, ring, transpose, colour)`, and
/// `edits` (raw values / 128 unless a choice).
fn v2patch(oscs: &[(u8, u8, u8, u8)], edits: &[(usize, f32)]) -> Vec<u8> {
    let r = |x: u8| x as f32 / 128.0;
    let mut p = pm::DEFAULTS;
    p[v2::VOICE] = 1.0;
    p[pm::env(0, e::MODE)] = env_mode::encode(env_mode::V2);
    (p[pm::env(0, e::ATTACK)], p[pm::env(0, e::DECAY)], p[pm::env(0, e::SUSTAIN)], p[pm::env(0, e::SLOPE)]) = (0.0, 0.5, 1.0, 0.5);
    p[pm::env(0, e::RELEASE)] = r(40);
    p[pm::env_amp(0)] = 1.0;
    p[v::PAN] = 0.5;
    for (i, &(mode, ring, tr, col)) in oscs.iter().enumerate() {
        p[pm::osc(i, o::TYPE)] = osc_type::encode(osc_type::V2);
        p[pm::osc(i, o::SHAPE + shape::V2_MODE)] = r(mode);
        p[pm::osc(i, o::SHAPE + shape::V2_RING)] = r(ring);
        p[pm::osc(i, o::COARSE)] = r(tr);
        p[pm::osc(i, o::FINE)] = 0.5;
        p[pm::osc(i, o::SHAPE + shape::V2_COLOR)] = r(col);
        p[pm::osc(i, o::GAIN)] = 1.0;
    }
    for &(i, x) in edits {
        p[i] = x;
    }
    encode(&p, &[])
}

fn play(chunk: &[u8], note: u8, n: usize) -> (Vec<f32>, Vec<f32>) {
    with_ins(chunk, 44100.0, |ins| render(ins, 44100.0, &[Ev { frame: 0, note, vel: 100 }], n, 512))
}

/// Magnitude of the DFT bin nearest `hz` over `x`.
fn mag(x: &[f32], hz: f64) -> f64 {
    let (mut re, mut im) = (0.0, 0.0);
    for (i, s) in x.iter().enumerate() {
        let a = 2.0 * std::f64::consts::PI * hz * i as f64 / 44100.0;
        re += *s as f64 * a.cos();
        im += *s as f64 * a.sin();
    }
    (re * re + im * im).sqrt() / x.len() as f64
}

fn db(a: f64, b: f64) -> f64 {
    20.0 * (a.max(1e-12) / b.max(1e-12)).log10()
}

/// V2's oscillator note: MIDI 81 at transpose 64 sounds 440 Hz (V2's note 60 is 130.8 Hz).
const A440: u8 = 81;

#[test]
fn a_centred_pulse_is_a_square_with_odd_harmonics() {
    let (l, _) = play(&v2patch(&[(2, 0, 64, 64)], &[]), A440, 44100);
    let x = &l[4410..44100];
    let f1 = mag(x, 440.0);
    assert!(f1 > 0.05, "{f1}");
    assert!(db(mag(x, 880.0), f1) < -30.0, "even harmonic {}", db(mag(x, 880.0), f1));
    assert!(db(mag(x, 1320.0), f1) > -12.0, "odd harmonic {}", db(mag(x, 1320.0), f1));
}

#[test]
fn ring_mode_multiplies_into_the_sum() {
    // two sines an octave apart (440, 880), the second in ring mode: 440 and 1320 Hz, not the inputs alone
    let (l, _) = play(&v2patch(&[(3, 0, 64, 64), (3, 1, 76, 64)], &[]), A440, 44100);
    let x = &l[4410..44100];
    let (sum, diff, a, b) = (mag(x, 1320.0), mag(x, 440.0), mag(x, 880.0), mag(x, 1320.0));
    assert!(sum > 0.05 && diff > 0.05, "{sum} {diff}");
    assert!(db(a, sum) < -30.0, "880 Hz left: {}", db(a, sum));
    let _ = b;
}

#[test]
fn parallel_routing_balances_the_two_filters() {
    // slot 1 low-pass at a low cutoff, slot 2 high-pass high; balance fully to slot 2 = slot 2 alone
    let saw = [(1u8, 0u8, 64u8, 0u8)];
    let base = [
        (pm::filt(0, f::TYPE), ft::encode(ft::V2_LP)),
        (pm::filt(0, f::CUTOFF), 30.0 / 128.0),
        (pm::filt(1, f::TYPE), ft::encode(ft::V2_HP)),
        (pm::filt(1, f::CUTOFF), 110.0 / 128.0),
        (v2::ROUTING, 2.0 / 128.0),
    ];
    let mut par = base.to_vec();
    par.push((v2::BALANCE, 1.0)); // raw 128: x = 1 → filter 1 silent, filter 2 full
    let (a, _) = play(&v2patch(&saw, &par), 57, 20000);
    let mut alone = vec![(pm::filt(0, f::TYPE), ft::encode(ft::V2_HP)), (pm::filt(0, f::CUTOFF), 110.0 / 128.0)];
    alone.push((v2::ROUTING, 0.0));
    let (b, _) = play(&v2patch(&saw, &alone), 57, 20000);
    let d = a.iter().zip(&b).map(|(x, y)| (x - y).abs()).fold(0.0f32, f32::max);
    assert!(d < 1e-4, "parallel at full balance vs slot 2 alone: {d}");
}

#[test]
fn the_voice_distortion_drives_and_off_changes_nothing() {
    let sine = [(3u8, 0u8, 64u8, 64u8)];
    let (clean, _) = play(&v2patch(&sine, &[]), A440, 20000);
    let (off, _) = play(&v2patch(&sine, &[(pm::VDIST, 0.0), (pm::VDIST + pm::dist::GAIN, 1.0)]), A440, 20000);
    assert_eq!(clean, off, "mode off: the gain does nothing");
    // overdrive at input gain 1.41 (V2's arctangent is right there): odd harmonics appear, the peak stays below 1
    let od = [(pm::VDIST, 1.0 / 128.0), (pm::VDIST + pm::dist::GAIN, 40.0 / 128.0), (pm::VDIST + pm::dist::P1, 1.0), (pm::VDIST + pm::dist::P2, 0.5)];
    let (x, _) = play(&v2patch(&sine, &od), A440, 20000);
    let y = &x[2000..20000];
    assert!(y.iter().all(|s| s.abs() <= 1.0));
    assert!(db(mag(y, 1320.0), mag(y, 440.0)) > -45.0, "{}", db(mag(y, 1320.0), mag(y, 440.0)));
}

/// Pan after distortion (change unify-v2-algorithms 5.2): with the pan stage after the voice drive and the voice pan
/// hard left, the right channel is silent and the left carries the driven mono sum.
#[test]
fn pan_after_distortion() {
    let od = [(pm::VDIST, 1.0 / 128.0), (pm::VDIST + pm::dist::GAIN, 40.0 / 128.0), (pm::VDIST + pm::dist::P1, 1.0), (pm::VDIST + pm::dist::P2, 0.5), (v::PAN, 0.0)];
    let (l, r) = play(&v2patch(&[(3, 0, 64, 64)], &od), A440, 20000);
    assert!(r.iter().all(|x| x.abs() < 1e-6), "right is silent");
    let y = &l[2000..20000];
    assert!(mag(y, 440.0) > 0.05 && db(mag(y, 1320.0), mag(y, 440.0)) > -45.0, "left: the driven sine");
}

#[test]
fn the_v2_sustain_time_slopes_the_held_level() {
    let lvl = |slope: u8| {
        let (x, _) = play(&v2patch(&[(3, 0, 64, 64)], &[(pm::env(0, e::SUSTAIN), 0.5), (pm::env(0, e::SLOPE), slope as f32 / 128.0)]), A440, 88200);
        let rms = |a: &[f32]| (a.iter().map(|s| (s * s) as f64).sum::<f64>() / a.len() as f64).sqrt();
        (rms(&x[22050..26460]), rms(&x[79380..88200]))
    };
    let (a, b) = lvl(64);
    assert!((db(b, a)).abs() < 0.2, "centre: flat {}", db(b, a));
    let (a, b) = lvl(20);
    assert!(db(b, a) < -6.0, "below the centre the level falls: {}", db(b, a));
}

#[test]
fn an_lfo_in_envelope_mode_runs_one_cycle_and_holds() {
    // LFO 1 V2 mode, saw, key sync, one-shot, routed (Add) to oscillator 1's transpose: the pitch rises once, then holds
    let t = target::PARAM + target::PARAMS.iter().position(|&i| i == v3i(pm::osc(0, o::COARSE))).unwrap() as u8;
    let edits = [
        (pm::lfo_ext(0, lx::MODE), 1.0),
        (pm::lfo(0, l::SHAPE), 0.0),
        (pm::lfo(0, l::SYNC), 1.0 / 128.0),
        (pm::lfo_ext(0, lx::EG), 1.0 / 128.0),
        (pm::lfo(0, l::RATE), 90.0 / 128.0),
        (pm::lfo(0, l::DEPTH), 12.0 / 128.0),
        (pm::modslot(0, pm::m::SOURCE), source_old(source::LFO1)),
        (pm::modslot(0, pm::m::TARGET), inv::inv_index(t)),
        (pm::modslot(0, pm::m::OPERATION), op_old(op::ADD)),
        (pm::modslot(0, pm::m::AMOUNT), 1.0),
    ];
    let (x, _) = play(&v2patch(&[(3, 0, 64, 64)], &edits), 69, 88200);
    let zc = |a: &[f32]| a.windows(2).filter(|w| w[0] <= 0.0 && w[1] > 0.0).count();
    let (late1, late2) = (zc(&x[44100..66150]), zc(&x[66150..88200]));
    assert!(late1.abs_diff(late2) <= 1, "held after one cycle: {late1} vs {late2}");
    assert!(late1 > zc(&x[0..2205]) * 10, "the pitch went up");
}

#[test]
fn the_channel_stage_volume_and_the_newest_voice_s_routes() {
    let sine = [(3u8, 0u8, 64u8, 64u8)];
    let (base, _) = play(&v2patch(&sine, &[]), A440, 20000);
    // channel stage on at volume 1: V2's 0.6 gain (its DC filters take a little more)
    let rms = |a: &[f32]| (a.iter().map(|s| (s * s) as f64).sum::<f64>() / a.len() as f64).sqrt();
    let (chan, _) = play(&v2patch(&sine, &[(ch::ON, 1.0), (ch::VOL, 1.0)]), A440, 20000);
    let ratio = rms(&chan[5000..20000]) / rms(&base[5000..20000]);
    assert!((ratio - 0.6).abs() < 0.01, "{ratio}");
    // velocity → channel volume (Add), from the newest voice: a quiet second note lowers the whole channel
    let t = target::PARAM + target::PARAMS.iter().position(|&i| i == v3i(ch::VOL)).unwrap() as u8;
    let edits = [
        (ch::ON, 1.0),
        (ch::VOL, 0.0),
        (pm::modslot(0, pm::m::SOURCE), source_old(source::VELOCITY)),
        (pm::modslot(0, pm::m::TARGET), inv::inv_index(t)),
        (pm::modslot(0, pm::m::OPERATION), op_old(op::ADD)),
        (pm::modslot(0, pm::m::AMOUNT), 1.0),
    ];
    let (x, _) = with_ins(&v2patch(&sine, &edits), 44100.0, |ins| {
        render(ins, 44100.0, &[Ev { frame: 0, note: A440, vel: 127 }, Ev { frame: 22050, note: 57, vel: 16 }], 44100, 512)
    });
    assert!(db(rms(&x[24000..44100]), rms(&x[2000..22000])) < -10.0, "the quiet note sets the channel volume");
}

/// V2's aux oscillator (format version 6, `us-strip`): it plays the aux bus it reads, (l + r) · gain · 0.6, through the
/// voice; aux A and aux B are the instrument's extra input pairs 1 and 2.
#[cfg(feature = "us-strip")]
#[test]
fn an_aux_oscillator_plays_its_aux_bus() {
    use synth_core::device::{EXT_CH, OUT_PAIRS};
    let n = 8192;
    let sine: Vec<f32> = (0..n).map(|i| (2.0 * std::f32::consts::PI * 300.0 * i as f32 / 44100.0).sin() * 0.25).collect();
    // render with `bus` (0 = A, 1 = B) carrying the sine on both channels
    let run = |mode: u8, bus: usize| {
        with_ins(&v2patch(&[(mode, 0, 64, 64)], &[]), 44100.0, |ins| {
            ins.note_on(60, 100, 0);
            let mut ext: Vec<Vec<f32>> = vec![vec![0.0; n]; EXT_CH];
            ext[2 * OUT_PAIRS + 2 * bus] = sine.clone();
            ext[2 * OUT_PAIRS + 2 * bus + 1] = sine.clone();
            let (mut a, mut b, mut c, mut d) = (vec![0.0; n], vec![0.0; n], vec![0.0; n], vec![0.0; n]);
            let mut refs: Vec<&mut [f32]> = ext.iter_mut().map(|x| x.as_mut_slice()).collect();
            let mut rng = Lcg(1);
            let mut ctx = Ctx { rate: 44100.0, tempo: 120, frame: 0, rng: &mut rng };
            ins.run_ext(&mut Io { ch: [&mut a, &mut b, &mut c, &mut d] }, &mut refs, n, &mut ctx);
            a
        })
    };
    let a = run(6, 0);
    // past the attack: (sine + sine) · 0.6 · envelope 1 · V2's centre pan (√½), the DC filter barely touching 300 Hz
    let (got, want) = (mag(&a[2048..n], 300.0), mag(&sine[2048..n], 300.0) * 2.0 * 0.6 * std::f64::consts::FRAC_1_SQRT_2);
    assert!(db(got, want).abs() < 0.5, "{got} vs {want}");
    assert!(mag(&run(7, 1)[2048..n], 300.0) > 0.9 * got, "aux B reads input pair 2");
    assert!(mag(&run(6, 1)[2048..n], 300.0) < 1e-6, "aux A ignores aux B");
}

/// An aux oscillator needs the extra input pairs, so `needs` names the channel stage's module for it.
#[cfg(feature = "std")]
#[test]
fn an_aux_oscillator_needs_the_channel_stage() {
    let c = v2patch(&[(6, 0, 64, 64)], &[]);
    let p = crate::chunk::Chunk::parse(&c).unwrap().values();
    assert!(crate::osc::v2_aux(&p, 0));
    let why = crate::needs::instrument_why(&c, &|_| None).unwrap();
    assert!(why.contains_key(&crate::module::STRIP), "{why:?}");
    // before version 6 V2 had no aux buses: the mode is silent and needs nothing
    let old = v2patch(&[(6, 0, 64, 64)], &[(v2::ERA, pm::kinds::encode(5, 7))]);
    assert!(!crate::osc::v2_aux(&crate::chunk::Chunk::parse(&old).unwrap().values(), 0));
}

// --- mixed patches: V2 modules in a unisynth voice (`v2.voice` off), and unisynth modules beside V2 slots ---

/// A unisynth voice (`v2.voice` off) with a BLIT oscillator in slot 0, plus `edits`.
fn mixed(edits: &[(usize, f32)]) -> Vec<u8> {
    let mut p = pm::DEFAULTS;
    p[pm::osc(0, o::TYPE)] = osc_type::encode(osc_type::BLIT);
    for &(i, x) in edits {
        p[i] = x;
    }
    encode(&p, &[])
}

fn rms(x: &[f32]) -> f64 {
    (x.iter().map(|&s| (s as f64) * (s as f64)).sum::<f64>() / x.len() as f64).sqrt()
}

#[test]
fn mix_v2_moog_filter_in_a_unisynth_voice() {
    let (dry, _) = play(&mixed(&[]), 57, 44100);
    let (wet, _) = play(&mixed(&[(pm::filt(0, f::TYPE), ft::encode(ft::V2_MOOG_LP)), (pm::filt(0, f::CUTOFF), 40.0 / 128.0)]), 57, 44100);
    let (d, w) = (&dry[4410..], &wet[4410..]);
    eprintln!("moog: h1 {:.1} dB, h20 {:.1} dB", db(mag(w, 220.0), mag(d, 220.0)), db(mag(w, 4400.0), mag(d, 4400.0)));
    assert!(db(mag(w, 4400.0), mag(d, 4400.0)) < db(mag(w, 220.0), mag(d, 220.0)) - 20.0);
}

#[test]
fn mix_v2_distortion_in_a_unisynth_voice() {
    let (dry, _) = play(&mixed(&[]), 57, 22050);
    // overdrive, in gain 120, out gain (param 1) 100: V2's default out gain 0 is silent, as in V2
    let (wet, _) = play(&mixed(&[(pm::VDIST + pm::dist::MODE, 1.0 / 128.0), (pm::VDIST + pm::dist::GAIN, 120.0 / 128.0), (pm::VDIST + pm::dist::P1, 100.0 / 128.0)]), 57, 22050);
    let pk = wet.iter().fold(0f32, |a, &x| a.max(x.abs()));
    eprintln!("vdist: dry rms {:.3} wet rms {:.3} peak {pk:.3}", rms(&dry), rms(&wet));
    assert!(rms(&wet) > 0.01 && (rms(&wet) - rms(&dry)).abs() > 0.01 && pk < 2.0);
}

#[test]
fn mix_v2_envelope_in_a_unisynth_voice() {
    let held = |slope: f32| {
        let c = mixed(&[(pm::env(0, e::MODE), env_mode::encode(env_mode::V2)), (pm::env(0, e::ATTACK), 0.0), (pm::env(0, e::DECAY), 0.5), (pm::env(0, e::SUSTAIN), 1.0), (pm::env(0, e::SLOPE), slope), (pm::env_amp(0), 1.0)]);
        let (l, _) = play(&c, 57, 88200);
        (rms(&l[4410..8820]), rms(&l[79380..88200]))
    };
    let (a, b) = held(0.5);
    let (c, d) = held(0.3);
    eprintln!("env: flat {a:.3}->{b:.3}, falling {c:.3}->{d:.3}");
    assert!(a > 0.01 && (b / a - 1.0).abs() < 0.05 && d < c * 0.7);
}

#[test]
fn mix_parallel_routing_of_unisynth_filters() {
    let lp = mixed(&[(pm::filt(0, f::TYPE), ft::encode(ft::SVF_LP)), (pm::filt(0, f::CUTOFF), 0.3), (pm::filt(1, f::TYPE), ft::encode(ft::SVF_HP)), (pm::filt(1, f::CUTOFF), 0.7)]);
    let par = mixed(&[(pm::filt(0, f::TYPE), ft::encode(ft::SVF_LP)), (pm::filt(0, f::CUTOFF), 0.3), (pm::filt(1, f::TYPE), ft::encode(ft::SVF_HP)), (pm::filt(1, f::CUTOFF), 0.7), (v2::ROUTING, 2.0 / 128.0), (v2::BALANCE, 0.5)]);
    let (s, _) = play(&lp, 57, 44100);
    let (q, _) = play(&par, 57, 44100);
    eprintln!("route: serial rms {:.4} parallel rms {:.4}", rms(&s[4410..]), rms(&q[4410..]));
    assert!(rms(&q[4410..]) > 4.0 * rms(&s[4410..]));
}

#[test]
fn mix_v2_oscillator_with_unisynth_filter_and_voice_pan() {
    let mut p = pm::DEFAULTS;
    p[pm::osc(0, o::TYPE)] = osc_type::encode(osc_type::V2);
    p[pm::osc(0, o::SHAPE + shape::V2_MODE)] = 1.0 / 128.0;
    p[pm::osc(0, o::COARSE)] = 0.5;
    p[pm::osc(0, o::FINE)] = 0.5;
    p[pm::osc(0, o::GAIN)] = 1.0;
    p[pm::filt(0, f::TYPE)] = ft::encode(ft::SVF_LP);
    p[pm::filt(0, f::CUTOFF)] = 0.4;
    let c = encode(&p, &[]);
    let (l, r) = play(&c, A440, 44100);
    p[v::PAN] = 0.0;
    let (pl, pr) = play(&encode(&p, &[]), A440, 44100);
    eprintln!("v2 osc: centre l {:.3} r {:.3}; pan left l {:.3} r {:.3}", rms(&l), rms(&r), rms(&pl), rms(&pr));
    // centred: the unisynth pan law (√½ per side), like a unisynth slot; hard left: nothing on the right
    assert!((rms(&l) - rms(&r)).abs() < 1e-3 && rms(&pr) < 0.01 * rms(&pl));
}
