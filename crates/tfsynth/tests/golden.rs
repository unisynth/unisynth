//! Self-golden: FNV-1a 64 of the f32 output of each in-repo synthetic patch playing `PHRASE`, kept in
//! `tests/golden.txt`. `UPDATE_GOLDEN=1 cargo test -p tfsynth --test golden` rewrites it (review the diff).

mod patches;

use synth_core::pool::Pool;
use tfsynth::fx::{Chain, Fx};
use tfsynth::instrument::Instrument;
use tfsynth::voice::BLOCK;

fn fnv(x: &[f32]) -> u64 {
    x.iter().flat_map(|v| v.to_bits().to_le_bytes()).fold(0xcbf29ce484222325u64, |h, b| (h ^ b as u64).wrapping_mul(0x100000001b3))
}

fn play(p: &[f32; tfsynth::params::COUNT]) -> Vec<f32> {
    let fx = Chain::need(p);
    let mut mem = vec![0u8; Instrument::need(fx)];
    let mut pool = Pool::new(&mut mem);
    let mut ins = Instrument::new(&mut pool, fx).unwrap();
    ins.set_patch(p).unwrap();
    let mut out = [[0.0f32; BLOCK]; 2];
    let mut v = vec![];
    for b in 0..patches::PHRASE_BLOCKS {
        for &(at, note, vel) in &patches::PHRASE {
            if at == b {
                ins.note_on(note, vel); // velocity 0 → note-off
            }
        }
        if b == 40 {
            ins.mod_wheel(0.7);
            ins.pitch_bend(0.3);
        }
        ins.render(&mut out);
        for i in 0..BLOCK {
            v.push(out[0][i]);
            v.push(out[1][i]);
        }
    }
    v
}

fn line(name: &str, x: &[f32]) -> String {
    let peak = x.iter().fold(0.0f32, |m, v| m.max(v.abs()));
    format!("{name} {:016x} peak={peak:.4}", fnv(x))
}

#[test]
fn golden() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/golden.txt");
    let mut got: Vec<String> = patches::instrument_patches().iter().map(|(n, p)| line(n, &play(p))).collect();
    // effect goldens only for kinds compiled in (a disabled kind would act as none and change the hash)
    let effects: Vec<_> = patches::effect_patches().into_iter().filter(|(_, k, _)| enabled(*k)).collect();
    got.extend(effects.iter().map(|(n, _, p)| line(n, &play(p))));
    for l in &got {
        assert!(!l.contains("peak=0.0000") && !l.contains("NaN"), "{l}");
    }
    let text = got.join("\n") + "\n";
    if std::env::var_os("UPDATE_GOLDEN").is_some() {
        std::fs::write(path, &text).unwrap();
        return;
    }
    let want = std::fs::read_to_string(path).expect("tests/golden.txt missing: run with UPDATE_GOLDEN=1");
    for g in &got {
        let name = g.split(' ').next().unwrap();
        let w = want.lines().find(|w| w.split(' ').next() == Some(name)).unwrap_or_else(|| panic!("{name} missing from golden.txt"));
        assert_eq!(g, w);
    }
}

/// Whether effect `kind` is compiled in (it then needs pool memory or is one of the pool-free kinds that exist).
fn enabled(kind: u8) -> bool {
    let mut mem = vec![0u8; Fx::need(kind) + 64];
    let mut pool = Pool::new(&mut mem);
    !matches!(Fx::create(kind, &mut pool, &mut tfsynth::rng::Xorshift::new()), Ok(Fx::None))
}
