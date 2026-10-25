//! Self-golden: FNV-1a 64 of every synthetic song's f32 output (bit-exact) in `tests/golden.txt`.
//! `UPDATE_GOLDEN=1 cargo test -p wavesabre --test golden` rewrites it; review the diff before committing.

mod songs;

use synth_core::mix::{Env, Player, Song, DEFAULT_SUB};
use synth_core::pool::Pool;
use wavesabre::helpers::WsRng;
use wavesabre::WsDevice;

fn render(blob: &[u8], sub: usize) -> Vec<f32> {
    let s = Song::parse(blob).unwrap();
    let env = Env::for_song(&s, None);
    let need = Player::<WsDevice, WsRng>::required_pool(&s, &env, sub).unwrap();
    let mut mem = vec![0u8; need];
    let mut pool = Pool::new(&mut mem);
    let mut p = Player::<WsDevice, WsRng>::new(s, &env, &mut pool, WsRng::new(), sub).unwrap();
    let mut out = vec![0.0f32; 2 * p.length_frames() as usize];
    // odd caller sizes: output must not depend on them
    for c in out.chunks_mut(2 * 777) {
        p.render_f32(c);
    }
    out
}

fn fnv(x: &[f32]) -> u64 {
    x.iter().flat_map(|v| v.to_bits().to_le_bytes()).fold(0xcbf29ce484222325u64, |h, b| {
        (h ^ b as u64).wrapping_mul(0x100000001b3)
    })
}

fn line(name: &str, x: &[f32]) -> String {
    let peak = x.iter().fold(0.0f32, |m, v| m.max(v.abs()));
    let rms = (x.iter().map(|v| (*v as f64).powi(2)).sum::<f64>() / x.len() as f64).sqrt();
    format!("{name} {:016x} peak={peak:.4} rms={rms:.5}", fnv(x))
}

#[test]
fn golden() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/golden.txt");
    let got: Vec<String> = songs::all().iter().map(|(n, b)| line(n, &render(b, DEFAULT_SUB))).collect();
    for l in &got {
        let name = l.split(' ').next().unwrap();
        let silent_ok = name == "adultery_no_bank";
        assert_eq!(l.contains("peak=0.0000"), silent_ok, "{l}");
        assert!(!l.contains("NaN") && !l.contains("inf"), "{l}");
    }
    let text = got.join("\n") + "\n";
    if std::env::var_os("UPDATE_GOLDEN").is_some() {
        std::fs::write(path, &text).unwrap();
        return;
    }
    let want = std::fs::read_to_string(path).expect("tests/golden.txt missing: run with UPDATE_GOLDEN=1");
    for (g, w) in got.iter().zip(want.lines()) {
        assert_eq!(g, w);
    }
    assert_eq!(got.len(), want.lines().count());
}

#[test]
fn renders_are_bit_identical() {
    for (name, b) in songs::all() {
        assert_eq!(fnv(&render(&b, DEFAULT_SUB)), fnv(&render(&b, DEFAULT_SUB)), "{name}");
    }
}

fn player<'a>(blob: &'a [u8], mem: &'a mut Vec<u8>) -> Player<'a, WsDevice<'a>, WsRng> {
    let s = Song::parse(blob).unwrap();
    let env = Env::for_song(&s, None);
    *mem = vec![0u8; Player::<WsDevice, WsRng>::required_pool(&s, &env, DEFAULT_SUB).unwrap()];
    let mut pool = Pool::new(mem);
    Player::new(s, &env, &mut pool, WsRng::new(), DEFAULT_SUB).unwrap()
}

/// Spec "Reset equals fresh load": seek(0) resets every device and re-seeds the random source, so a second render
/// from the start is bit-identical to the first, for every device kind.
#[test]
fn reset_equals_fresh_load_for_every_device() {
    for (name, b) in songs::all() {
        let mut mem = vec![];
        let mut p = player(&b, &mut mem);
        let mut first = vec![0.0f32; 2 * p.length_frames() as usize];
        p.render_f32(&mut first);
        p.seek(0);
        let mut second = vec![0.0f32; first.len()];
        p.render_f32(&mut second);
        assert!(first == second, "{name}");
    }
}

/// Spec "Two songs side by side": two players with different tempos, interleaved call by call, each equal alone.
#[test]
fn two_songs_interleaved_equal_alone() {
    let (_, echo) = songs::all().into_iter().find(|(n, _)| *n == "echo").unwrap();
    let mut slow = echo.clone();
    slow[..4].copy_from_slice(&90i32.to_le_bytes()); // tempo: Echo's tap times follow it
    let alone_fast = render(&echo, DEFAULT_SUB);
    let alone_slow = render(&slow, DEFAULT_SUB);
    assert_ne!(fnv(&alone_fast), fnv(&alone_slow));
    let (mut ma, mut mb) = (vec![], vec![]);
    let (mut a, mut b) = (player(&echo, &mut ma), player(&slow, &mut mb));
    let (mut oa, mut ob) = (vec![0.0f32; alone_fast.len()], vec![0.0f32; alone_slow.len()]);
    for (ca, cb) in oa.chunks_mut(2 * 333).zip(ob.chunks_mut(2 * 333)) {
        a.render_f32(ca);
        b.render_f32(cb);
    }
    assert!(oa == alone_fast && ob == alone_slow);
}

/// Spec "Pool exhaustion": a pool one byte smaller than what loading actually consumes (same base address, so the
/// same alignment padding) fails with `OutOfMemory`, no panic. (`required_pool` is an upper bound incl. padding.)
#[test]
fn pool_one_byte_short_is_out_of_memory() {
    use synth_core::mix::LoadError;
    for (name, b) in songs::all() {
        let s = Song::parse(&b).unwrap();
        let env = Env::for_song(&s, None);
        let need = Player::<WsDevice, WsRng>::required_pool(&s, &env, DEFAULT_SUB).unwrap();
        let mut mem = vec![0u8; need];
        let used = {
            let mut pool = Pool::new(&mut mem);
            Player::<WsDevice, WsRng>::new(s, &env, &mut pool, WsRng::new(), DEFAULT_SUB).unwrap();
            need - pool.remaining()
        };
        let mut pool = Pool::new(&mut mem[..used - 1]);
        let r = Player::<WsDevice, WsRng>::new(s, &env, &mut pool, WsRng::new(), DEFAULT_SUB);
        assert!(matches!(r, Err(LoadError::OutOfMemory)), "{name}");
    }
}
