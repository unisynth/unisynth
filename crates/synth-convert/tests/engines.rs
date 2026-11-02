//! The unified oscillator tables, filters and noise band against the original engines' code, sample for sample on the same
//! input. The converter owns this check because it is the one crate that knows both sides.

use unisynth::flt::{self, Kind, Slot};
use unisynth::noise::{Noise, BAND_RES, HP, LP};
use unisynth::wt::{Shape, Table};

const RATE: f32 = 44100.0;

/// The shapes of the wavetable check: (harmonics, bandwidth, damp, scale, bins).
fn shapes() -> Vec<(usize, f32, f32, f32, usize)> {
    vec![(1, 0.0, 0.0, 0.0, 512), (33, 0.0, 0.0, 0.125, 246), (65, 0.3, 0.5, 0.25, 64), (12, 1.0, 1.0, 4.0, 512), (5, 0.05, 0.2, 0.5, 4)]
}

/// Deterministic white-ish input.
fn noise(n: usize) -> Vec<f32> {
    let mut s = 0x1234_5678u32;
    (0..n)
        .map(|_| {
            s ^= s << 13;
            s ^= s >> 17;
            s ^= s << 5;
            (s >> 8) as f32 / 8_388_608.0 - 1.0
        })
        .collect()
}

fn run(kind: Kind, hz: f32, res: f32, rate: f32, x: &[f32]) -> Vec<f32> {
    let mut f = Slot::new();
    f.set(kind, &flt::coefs(kind, hz, res, rate, flt::Forms::PLAIN));
    x.iter().map(|v| f.process(*v, *v).0).collect()
}

fn worst(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b).fold(0.0f32, |m, (x, y)| m.max((x - y).abs()))
}

#[test]
fn the_band_limit_of_the_tables_is_the_tunefish_one() {
    assert_eq!(unisynth::wt::band_bins(440.0, 44100.0), tfsynth::wavetable::band_bins(440.0));
}

#[test]
fn tables_match_the_tunefish_generator() {
    unisynth::tables::init();
    for (h, b, d, s, k) in shapes() {
        for (g, mu0) in [(0.0f32, 0.0f32), (0.0, 37.5), (0.5, 12.0), (1.0, 99.9)] {
            let mut ours = Table::new();
            let mut theirs = tfsynth::wavetable::Table::new();
            ours.update(&Shape { harmonics: h, bandwidth: b, damp: d, scale: s, bins: k });
            theirs.update(&tfsynth::wavetable::Shape { harmonics: h, bandwidth: b, damp: d, scale: s, bins: k });
            let (mut m1, mut m2) = (mu0, mu0);
            for round in 0..3 {
                ours.render(g, &mut m1);
                theirs.render(g, &mut m2);
                assert!((m1 - m2).abs() < 1e-4, "drift after {round}: {m1} vs {m2}");
                let worst = ours.wave().iter().zip(theirs.wave().iter()).fold(0.0f32, |m, (a, b)| m.max((a - b).abs()));
                assert!(worst < 1e-4, "shape {h} {b} {d} {s} {k}, g {g}, mu {mu0}, round {round}: {worst}");
            }
        }
    }
}

#[test]
fn ladder_rbj_and_notch_match_the_tunefish_filters() {
    let x = noise(8000);
    for (f, q) in [(0.1f32, 0.0f32), (0.4, 0.3), (0.7, 0.6), (1.0, 0.85), (0.5, 0.95)] {
        let fc = |scale: f32| f * f * scale + 30.0;
        let (mut l, mut r) = (x.clone(), x.clone());
        let mut lad = tfsynth::filter::Ladder::default();
        lad.set(f, q);
        lad.run(&mut l, &mut r);
        assert!(worst(&l, &run(Kind::Pole4Lp, fc(20000.0), q, RATE, &x)) < 1e-4, "ladder {f} {q}");
        for (rk, kind) in [(tfsynth::filter::RbjKind::HighPass, Kind::RbjHp), (tfsynth::filter::RbjKind::BandPass, Kind::RbjBp)] {
            let (mut l, mut r) = (x.clone(), x.clone());
            let mut b = tfsynth::filter::Rbj::default();
            b.set(rk, f, q);
            b.run(&mut l, &mut r);
            assert!(worst(&l, &run(kind, fc(10000.0), q, RATE, &x)) < 1e-4, "{kind:?} {f} {q}");
        }
        let (mut l, mut r) = (x.clone(), x.clone());
        let mut n = tfsynth::filter::Notch::default();
        n.set(f, q);
        n.run(&mut l, &mut r);
        assert!(worst(&l, &run(Kind::Notch, fc(20000.0), q, RATE, &x)) < 1e-4, "notch {f} {q}");
    }
}

/// The band path against the original filters (Tunefish's noise: ladder low-pass, then RBJ high-pass, resonance 0.05)
/// fed with the same samples.
#[test]
fn the_band_is_tunefishs_ladder_low_pass_then_rbj_high_pass() {
    let (lp_hz, hp_hz) = (1100.0f32, 900.0f32);
    let mut n = Noise::new();
    n.start(77);
    n.lp.set(LP, &unisynth::flt::coefs(LP, lp_hz, BAND_RES, RATE, unisynth::flt::Forms::PLAIN));
    n.hp.set(HP, &unisynth::flt::coefs(HP, hp_hz, BAND_RES, RATE, unisynth::flt::Forms::PLAIN));
    let mut raw = Noise::new();
    raw.start(77);
    let (mut l, mut r): (Vec<f32>, Vec<f32>) = (0..6000).map(|_| raw.next(2.0, false, 1.0)).unzip();
    let got: Vec<f32> = (0..6000).map(|_| n.next(2.0, true, 1.0).0).collect();
    let mut lad = tfsynth::filter::Ladder::default();
    assert_eq!(BAND_RES, 0.05, "Tunefish's band resonance");
    lad.set(((lp_hz - 30.0) / 20000.0).sqrt(), 0.05);
    let mut hp = tfsynth::filter::Rbj::default();
    hp.set(tfsynth::filter::RbjKind::HighPass, ((hp_hz - 30.0) / 10000.0).sqrt(), 0.05);
    lad.run(&mut l, &mut r);
    hp.run(&mut l, &mut r);
    let worst = got.iter().zip(&l).fold(0.0f32, |m, (a, b)| m.max((a - b).abs()));
    assert!(worst < 1e-3, "{worst}");
    assert!(got.iter().any(|v| v.abs() > 0.01));
}
