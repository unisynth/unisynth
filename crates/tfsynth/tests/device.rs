//! `Device` adapters on `synth-mix` (task 4.7): the instrument (kind 32) on a shared track, standalone effects
//! (33–39), mixed-engine songs through `Either`, reset, caller block sizes, and the no-duplicated-primitives check.

#[path = "../../wavesabre/tests/songs.rs"]
#[allow(dead_code)]
mod ws_songs;
mod patches;

use synth_core::device::{Ctx, Device, Io};
use synth_core::mix::{Either, Env, Kinds, LoadError, Player, Song, DEFAULT_SUB};
use synth_core::pool::Pool;
use tfsynth::device::{kind, TfDevice, TfEffect};
use tfsynth::fx::Chain;
use tfsynth::instrument::Instrument;
use tfsynth::params::*;
use tfsynth::rng::Xorshift;
use tfsynth::voice::BLOCK;
use wavesabre::helpers::WsRng;
use wavesabre::WsDevice;
use ws_songs::{chunk, song, Track};

fn render<'a, K: Kinds<'a> + 'a>(blob: &'a [u8], sizes: &[usize]) -> Vec<f32> {
    let s = Song::parse(blob).unwrap();
    let env = Env::for_song(&s, None);
    let need = Player::<K, WsRng>::required_pool(&s, &env, DEFAULT_SUB).unwrap();
    let mem: &'a mut [u8] = Box::leak(vec![0u8; need].into_boxed_slice());
    let mut pool = Pool::new(mem);
    let mut p = Player::<K, WsRng>::new(s, &env, &mut pool, WsRng::new(), DEFAULT_SUB).unwrap();
    let mut out = vec![0.0f32; 2 * p.length_frames() as usize];
    let mut i = 0;
    for &n in sizes.iter().cycle() {
        if i >= out.len() {
            break;
        }
        let end = (i + 2 * n).min(out.len());
        p.render_f32(&mut out[i..end]);
        i = end;
    }
    out
}

fn tf_patch() -> [f32; COUNT] {
    let mut p = patches::effect_patches().into_iter().find(|(n, _, _)| *n == "fx_chorus").unwrap().2;
    p[GEN_MOD] = 0.5;
    p[GEN_SLOP] = 0.4;
    p
}

/// Lane events (absolute frame, note, velocity; 0 = off) → `synth-mix` delta lane.
fn lane(ev: &[(u32, u8, u8)]) -> Vec<(i32, u8, Option<u8>)> {
    let mut prev = 0;
    ev.iter()
        .map(|&(f, n, v)| {
            let d = (f - prev) as i32;
            prev = f;
            (d, n, if v == 0 { None } else { Some(v) })
        })
        .collect()
}

const EVENTS: [(u32, u8, u8); 6] = [(0, 60, 100), (1000, 64, 90), (1023, 67, 80), (1024, 72, 100), (20000, 60, 0), (30000, 64, 0)];

/// Spec "Instrument as a shared device": events fire at the start of the absolute 512-frame block containing them,
/// so the device equals a direct render that fires each block's events first — for any caller block size.
#[test]
fn instrument_device_fires_events_at_block_starts() {
    let p = tf_patch();
    let blob = song(1.2, &[(kind::INSTRUMENT, chunk(&p))], &[lane(&EVENTS)], &[Track::new(&[0], 0)]);
    let blob: &'static [u8] = Box::leak(blob.into_boxed_slice());
    let a = render::<TfDevice>(blob, &[4096]);
    assert_eq!(a, render::<TfDevice>(blob, &[100, 777, 1]), "caller block size independent");

    let fx = Chain::need(&p);
    let mem: &'static mut [u8] = Box::leak(vec![0u8; Instrument::need(fx)].into_boxed_slice());
    let pool: &'static mut Pool<'static> = Box::leak(Box::new(Pool::new(mem)));
    let mut ins = Instrument::new(pool, fx).unwrap();
    ins.set_patch(&p).unwrap();
    let mut want = vec![];
    let mut buf = [[0.0f32; BLOCK]; 2];
    for b in 0..a.len() / 2 / BLOCK + 1 {
        for &(f, n, v) in &EVENTS {
            if f as usize / BLOCK == b {
                ins.note_on(n, v);
            }
        }
        ins.render(&mut buf);
        for i in 0..BLOCK {
            want.push(buf[0][i]);
            want.push(buf[1][i]);
        }
    }
    assert!(a == want[..a.len()], "device = direct render with block-start events");
}

#[test]
fn reset_equals_fresh_load() {
    let blob = song(1.0, &[(kind::INSTRUMENT, chunk(&tf_patch()))], &[lane(&EVENTS)], &[Track::new(&[0], 0)]);
    let blob: &'static [u8] = Box::leak(blob.into_boxed_slice());
    let s = Song::parse(blob).unwrap();
    let env = Env::for_song(&s, None);
    let need = Player::<TfDevice, WsRng>::required_pool(&s, &env, DEFAULT_SUB).unwrap();
    let mem: &'static mut [u8] = Box::leak(vec![0u8; need].into_boxed_slice());
    let mut pool = Pool::new(mem);
    let mut p = Player::<TfDevice, WsRng>::new(s, &env, &mut pool, WsRng::new(), DEFAULT_SUB).unwrap();
    let mut a = vec![0.0f32; 2 * p.length_frames() as usize];
    p.render_f32(&mut a);
    p.seek(0);
    let mut b = vec![0.0f32; a.len()];
    p.render_f32(&mut b);
    assert!(a == b);
}

fn run_effect(d: &mut TfEffect, x: &[f32], n: usize) -> (Vec<f32>, Vec<f32>) {
    let (mut l, mut r) = (x.to_vec(), x.iter().map(|v| -v * 0.5).collect::<Vec<_>>());
    let (mut s0, mut s1) = (vec![0.0f32; n], vec![0.0f32; n]);
    let mut rng = Xorshift::new();
    for (a, b) in l.chunks_mut(n).zip(r.chunks_mut(n)) {
        let m = a.len();
        let mut ctx = Ctx { rate: 44100.0, tempo: 120, frame: 0, rng: &mut rng };
        d.run(&mut Io { ch: [a, b, &mut s0[..m], &mut s1[..m]] }, m, &mut ctx);
    }
    (l, r)
}

fn effect(k: u8, params: &[f32]) -> TfEffect<'static> {
    let c: &'static [u8] = Box::leak(chunk(params).into_boxed_slice());
    let mem: &'static mut [u8] = Box::leak(vec![0u8; TfEffect::need(k) + 64].into_boxed_slice());
    TfEffect::new(k, c, &mut Pool::new(mem)).unwrap()
}

fn signal(n: usize) -> Vec<f32> {
    let mut s = 1u32;
    (0..n)
        .map(|i| {
            s = s.wrapping_mul(1664525).wrapping_add(1013904223);
            0.3 * ((i as f32) * 0.05).sin() + 0.2 * ((s >> 8) as f32 / 16777216.0 - 0.5)
        })
        .collect()
}

/// Spec "Caller block size": every standalone effect, blocks of 100 vs 512 vs 7, bit-identical.
#[test]
fn standalone_effects_ignore_caller_block_size() {
    let x = signal(44100);
    for (k, params) in [
        (kind::DISTORTION, vec![0.6]),
        (kind::DELAY, vec![0.05, 0.08, 0.5]),
        (kind::CHORUS, vec![0.7, 0.8, 0.9]),
        (kind::FLANGER, vec![0.3, 0.6, 0.8, 0.5]),
        (kind::REVERB, vec![0.6, 0.3, 0.4, 0.7]),
        (kind::FORMANT, vec![0.25, 0.8]),
        (kind::EQ, vec![0.8, 0.4, 0.6]),
    ] {
        let a = run_effect(&mut effect(k, &params), &x, 512);
        assert!(a == run_effect(&mut effect(k, &params), &x, 100), "kind {k}");
        assert!(a == run_effect(&mut effect(k, &params), &x, 7), "kind {k}");
        let mut d = effect(k, &params);
        assert_eq!(d.param_count(), params.len());
        let first = run_effect(&mut d, &x, 512);
        d.reset();
        assert!(first == run_effect(&mut d, &x, 512), "kind {k} reset");
    }
}

/// Spec "Standalone effect on another engine": a kind-37 reverb after a WaveSabre Slaughter equals the same reverb
/// (as inside a Tunefish instrument, 512-frame blocks) applied to the Slaughter's output.
#[test]
fn standalone_reverb_after_wavesabre_slaughter() {
    let rev = [0.6f32, 0.3, 0.4, 0.7];
    let mut sl = vec![0.0f32; 42];
    sl[2] = 0.8;
    sl[17] = 0.6;
    sl[22] = 0.7;
    sl[28] = 0.6;
    sl[31] = 0.5;
    let notes = lane(&[(0, 60, 100), (8000, 60, 0), (12000, 67, 100), (30000, 67, 0)]);
    let dry_blob = song(1.0, &[(wavesabre::kind::SLAUGHTER, chunk(&sl))], &[notes.clone()], &[Track::new(&[0], 0)]);
    let dry_blob: &'static [u8] = Box::leak(dry_blob.into_boxed_slice());
    let dry = render::<WsDevice>(dry_blob, &[4096]);
    let mixed = song(
        1.0,
        &[(wavesabre::kind::SLAUGHTER, chunk(&sl)), (kind::REVERB, chunk(&rev))],
        &[notes],
        &[Track::new(&[0], 0), Track::new(&[1], 0).recv(0, 0, 1.0)],
    );
    let mixed: &'static [u8] = Box::leak(mixed.into_boxed_slice());
    let got = render::<Either<WsDevice, TfDevice>>(mixed, &[100, 333]);
    // the same reverb through an instrument's effect chain (slot 1 = reverb), 512-frame blocks
    let mut p = [0.0f32; COUNT];
    p[EFFECT_FIRST] = 0.5;
    p[REV_ROOM..REV_ROOM + 4].copy_from_slice(&rev);
    let mem: &'static mut [u8] = Box::leak(vec![0u8; Chain::need(&p) + 64].into_boxed_slice());
    let mut chain = Chain::new(Pool::new(mem));
    let (mut l, mut r): (Vec<f32>, Vec<f32>) = (dry.iter().step_by(2).copied().collect(), dry.iter().skip(1).step_by(2).copied().collect());
    for (a, b) in l.chunks_mut(BLOCK).zip(r.chunks_mut(BLOCK)) {
        chain.run(&p, &mut Xorshift::new(), a, b);
    }
    let want: Vec<f32> = l.iter().zip(&r).flat_map(|(a, b)| [*a, *b]).collect();
    assert!(got == want);
    assert!(got.iter().any(|v| *v != 0.0));
}

#[test]
fn registry_kinds_and_lookahead() {
    let env = Env { rate: 44100.0, tempo: 120, bank: None };
    assert!(TfDevice::need(kind::INSTRUMENT, &[], &env).is_ok());
    for k in kind::DISTORTION..=kind::EQ {
        assert!(TfDevice::need(k, &[], &env).is_ok(), "{k}");
    }
    for k in [0u8, 31, 40, 255] {
        assert_eq!(TfDevice::need(k, &[], &env), Err(LoadError::UnsupportedKind(k)));
    }
    let mem: &'static mut [u8] = Box::leak(vec![0u8; TfDevice::need(kind::INSTRUMENT, &[], &env).unwrap()].into_boxed_slice());
    let d = TfDevice::load(kind::INSTRUMENT, &[], &env, &mut Pool::new(mem), &mut WsRng::new()).unwrap();
    assert_eq!((d.event_lookahead(), d.param_count()), (511, 112));
}

/// Spec "No duplicated primitives": tfsynth defines no comb, allpass, ring-buffer or biquad state of its own.
#[test]
fn no_duplicated_primitives() {
    fn walk(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
        for e in std::fs::read_dir(dir).unwrap() {
            let p = e.unwrap().path();
            if p.is_dir() {
                walk(&p, out);
            } else if p.extension().is_some_and(|x| x == "rs") {
                out.push(p);
            }
        }
    }
    let mut files = vec![];
    walk(&std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src"), &mut files);
    for f in files {
        let s = std::fs::read_to_string(&f).unwrap();
        for line in s.lines().filter(|l| l.trim_start().starts_with("pub struct") || l.trim_start().starts_with("struct")) {
            let name = line.split_whitespace().nth(if line.trim_start().starts_with("pub") { 2 } else { 1 }).unwrap_or("");
            for bad in ["Comb", "AllPass", "Allpass", "Ring", "Biquad", "OnePole"] {
                assert!(!name.contains(bad), "{}: {line}", f.display());
            }
        }
        assert!(!s.contains("x1: f32") && !s.contains("y2: f32"), "{}: biquad-style state", f.display());
    }
}
