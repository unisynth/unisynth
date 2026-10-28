//! Self-golden for the effects: FNV-1a 64 of the output of every mode on a fixed input (noise plus a sine, 12 000
//! frames, a call size that splits the chorus's control blocks), at 44.1 kHz / 120 BPM and at 48 kHz / 90 BPM, kept in
//! `tests/golden_fx.txt`. `UPDATE_GOLDEN=1 cargo test -p unisynth golden` rewrites it (review the diff). Modes the
//! build lacks are skipped.
#![allow(unused_imports)]

use super::tests::{mode, noise, run, sine, values, with_fx};
use super::*;

fn fnv(x: &[f32]) -> u64 {
    x.iter().flat_map(|v| v.to_bits().to_le_bytes()).fold(0xcbf29ce484222325u64, |h, b| (h ^ b as u64).wrapping_mul(0x100000001b3))
}

/// Parameter sets: a few settings that make each mode audible, by seed.
fn params(k: u8, i: usize) -> Vec<f32> {
    let mut p = vec![mode_value(k, i)];
    p.extend(values(500 + 10 * (k as u32 - 65) + i as u32, 14));
    p
}

#[test]
fn golden() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/golden_fx.txt");
    let mut got = vec![];
    for &(k, i, id) in MODES.iter() {
        let p = params(k, i);
        let chunk = encode(&p);
        let ok = |rate, tempo| UFx::need(k, &chunk, &Env { rate, tempo, bank: None }).is_ok();
        for (rate, tempo) in [(44100.0, 120), (48000.0, 90)] {
            if !ok(rate, tempo) {
                continue;
            }
            let (mut l, mut r) = noise(3 + id as u32, 12000);
            let tone = sine(330.0, rate as f32, 12000, 0.25);
            l.iter_mut().zip(&tone).for_each(|(a, b)| *a = *a * 0.5 + b);
            r.iter_mut().zip(&tone).for_each(|(a, b)| *a = *a * 0.5 - b);
            let (ol, or) = with_fx(k, &p, rate, tempo, |fx| run(fx, rate, tempo, &l, &r, 777));
            let out: Vec<f32> = ol.iter().zip(&or).flat_map(|(a, b)| [*a, *b]).collect();
            assert!(out.iter().all(|v| v.is_finite()), "kind {k} mode {i} at {rate}");
            let peak = out.iter().fold(0.0f32, |m, v| m.max(v.abs()));
            assert!(peak > 1e-4, "kind {k} mode {i} at {rate}: silent");
            got.push(format!("fx{k}.{i}@{}k{tempo} {:016x} peak={peak:.4}", rate as u32 / 1000, fnv(&out)));
        }
    }
    let text = got.join("\n") + "\n";
    if std::env::var_os("UPDATE_GOLDEN").is_some() {
        std::fs::write(path, &text).unwrap();
        return;
    }
    let want = std::fs::read_to_string(path).expect("tests/golden_fx.txt missing: run with UPDATE_GOLDEN=1");
    for g in &got {
        let name = g.split(' ').next().unwrap();
        let w = want.lines().find(|w| w.split(' ').next() == Some(name)).unwrap_or_else(|| panic!("{name} missing from golden_fx.txt"));
        assert_eq!(g, w);
    }
    if cfg!(feature = "us-fx") {
        assert_eq!(want.lines().count(), got.len(), "golden_fx.txt has modes this test no longer plays");
    }
}
