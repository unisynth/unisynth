//! `.tfm` decoding and playback (tasks 5.1–5.2) on songs built here, and the "Same timing on a shared track"
//! scenario (`.tfm` player vs the kind-32 device on a `synth-mix` track).

#[path = "../../wavesabre/tests/songs.rs"]
#[allow(dead_code)]
mod ws_songs;
mod patches;

use synth_core::mix::{Env, Player, Song, DEFAULT_SUB};
use synth_core::pool::Pool;
use tfsynth::device::{kind, TfDevice};
use tfsynth::params::*;
use tfsynth::player::TfmPlayer;
use tfsynth::tfm::{Event, Tfm, TfmError};
use tfsynth::voice::BLOCK;
use wavesabre::helpers::WsRng;

/// Writes a `.tfm`: `(patch, [(row delta, note, velocity)])` per instrument.
fn tfm(tempo: u16, ins: &[([f32; COUNT], Vec<(u16, u8, u8)>)]) -> Vec<u8> {
    let mut b = vec![];
    b.extend((ins.len() as u16).to_le_bytes());
    b.extend(tempo.to_le_bytes());
    for (_, ev) in ins {
        b.extend((ev.len() as u16).to_le_bytes());
    }
    b.extend(b"INST");
    for (p, _) in ins {
        b.extend(p.iter().map(|v| (v * 100.0).round() as u8)); // exact /100 values in our patches
    }
    b.extend(b"SONG");
    for (_, ev) in ins {
        ev.iter().for_each(|e| b.extend(e.0.to_le_bytes()));
        ev.iter().for_each(|e| b.push(e.1));
        ev.iter().for_each(|e| b.push(e.2));
    }
    b.extend(b"ENDS");
    b
}

/// A patch with every value on the 1/100 grid (as `.tfm` stores them).
fn quantised(p: [f32; COUNT]) -> [f32; COUNT] {
    p.map(|v| (v * 100.0).trunc() / 100.0)
}

fn lead() -> [f32; COUNT] {
    let mut p = patches::base();
    p[GEN_HARMONICS] = 0.3;
    p[GEN_MOD] = 0.4;
    p[ENV1..ENV1 + 5].copy_from_slice(&[0.1, 0.4, 0.6, 0.4, 0.5]);
    patches::route(&mut p, 0, 3, 0.5, 5);
    p[EFFECT_FIRST] = 0.3; // chorus
    p[CHORUS_RATE] = 0.4;
    p[CHORUS_DEPTH] = 0.6;
    p[CHORUS_GAIN] = 0.8;
    quantised(p)
}

fn bass() -> [f32; COUNT] {
    let mut p = patches::base();
    p[GEN_HARMONICS] = 0.15;
    p[GEN_OCTAVE] = 0.37;
    p[LP_ON] = 1.0;
    p[LP_CUT] = 0.3;
    quantised(p)
}

const LEAD: [(u16, u8, u8); 6] = [(0, 60, 100), (2, 60, 0), (0, 64, 90), (3, 64, 0), (1, 0, 100), (2, 67, 80)];

#[test]
fn decode_round_trip() {
    let b = tfm(125, &[(lead(), LEAD.to_vec()), (bass(), vec![(4, 36, 120), (4, 36, 0)])]);
    let s = Tfm::parse(&b).unwrap();
    assert_eq!((s.instruments(), s.tempo), (2, 125));
    assert_eq!(s.patch(0), lead());
    assert_eq!(s.patch(1)[GEN_OCTAVE], 0.37);
    let ev: Vec<Event> = s.events(0).collect();
    assert_eq!(ev.len(), 6);
    assert_eq!(ev[3], Event { row: 5, note: 64, velocity: 0 });
    assert_eq!(s.events(1).map(|e| e.row).collect::<Vec<_>>(), vec![4, 8]);
}

#[test]
fn malformed_songs_are_errors() {
    let good = tfm(120, &[(lead(), LEAD.to_vec()), (bass(), vec![(4, 36, 120)])]);
    for cut in 0..good.len() {
        assert!(Tfm::parse(&good[..cut]).is_err(), "cut at {cut}");
    }
    let tag = |off: usize| {
        let mut b = good.clone();
        b[off] = b'X';
        Tfm::parse(&b).err()
    };
    assert_eq!(tag(4 + 2 * 2), Some(TfmError::BadTag)); // INST
    assert_eq!(tag(good.len() - 4), Some(TfmError::BadTag)); // ENDS
    let mut big = good.clone();
    big[0..2].copy_from_slice(&33u16.to_le_bytes());
    assert_eq!(Tfm::parse(&big).err(), Some(TfmError::TooManyInstruments));
    let mut huge = good.clone();
    huge[4..6].copy_from_slice(&u16::MAX.to_le_bytes()); // event count past the end
    assert!(Tfm::parse(&huge).is_err());
    let mut tail = good.clone();
    tail.extend(b"junk");
    assert!(Tfm::parse(&tail).is_ok(), "bytes after ENDS are ignored");
}

fn player(b: &'static [u8], offset: f32) -> TfmPlayer<'static> {
    let s = Tfm::parse(b).unwrap();
    let mem: &'static mut [u8] = Box::leak(vec![0u8; TfmPlayer::required_pool(&s)].into_boxed_slice());
    let pool: &'static mut Pool<'static> = Box::leak(Box::new(Pool::new(mem)));
    TfmPlayer::new(s, pool, offset).unwrap()
}

fn leak(b: Vec<u8>) -> &'static [u8] {
    Box::leak(b.into_boxed_slice())
}

fn f32_blocks(p: &mut TfmPlayer, n: usize) -> Vec<f32> {
    let mut v = vec![];
    let mut out = [0.0f32; 2 * BLOCK];
    for _ in 0..n {
        p.render_f32(&mut out);
        v.extend_from_slice(&out);
    }
    v
}

/// Block index at which the player's f32 window first reaches time `t` (mirrors the reference's accumulation).
fn block_of(t: f32) -> usize {
    let (mut now, mut b) = (0.0f32, 0);
    loop {
        let next = now + BLOCK as f32 / 44100.0;
        if t >= now && t < next {
            return b;
        }
        now = next;
        b += 1;
    }
}

#[test]
fn events_fire_at_block_start() {
    // tempo 125: a row lasts 0.12 s = 5292 frames; row 1 lies 172 frames (3.9 ms) into block 10
    let b = leak(tfm(125, &[(lead(), vec![(1, 60, 100)])]));
    assert_eq!(block_of(0.12), 10);
    let mut p = player(b, 0.0);
    p.start(0.0);
    let out = f32_blocks(&mut p, 12);
    let first = out.iter().position(|v| *v != 0.0).unwrap() / 2;
    assert!(first >= 10 * BLOCK && first <= 10 * BLOCK + 1, "{first}"); // gain ramps from 0 at the block start
}

#[test]
fn note_zero_ignored_and_lengths() {
    let b = leak(tfm(120, &[(lead(), vec![(4, 0, 100), (4, 0, 0)])]));
    let mut p = player(b, 0.0);
    p.start(0.0);
    assert!(f32_blocks(&mut p, 50).iter().all(|v| *v == 0.0));
    // empty instrument skipped: length from the others; no events at all → 0
    let b = leak(tfm(120, &[(lead(), vec![]), (bass(), vec![(8, 36, 100), (8, 36, 0)])]));
    let p = player(b, 0.5);
    assert_eq!(p.length(), 16.0 * 60.0 / 480.0 + 0.5);
    assert_eq!(p.blocks(), (44100.0 * p.length() / 512.0) as usize + 1);
    let b = leak(tfm(120, &[(lead(), vec![]), (bass(), vec![])]));
    assert_eq!(player(b, 0.0).length(), 0.0);
}

#[test]
fn seek_releases_and_continues_mute_keeps_tails() {
    let ev = vec![(0, 60, 100), (16, 60, 0), (0, 64, 100), (16, 64, 0)];
    let b = leak(tfm(120, &[(lead(), ev)]));
    let mut p = player(b, 0.0);
    p.start(0.0);
    f32_blocks(&mut p, 20);
    assert!(p.instruments()[0].voices().iter().any(|v| v.held));
    p.seek(1.0);
    assert!(p.instruments()[0].voices().iter().all(|v| !v.held));
    assert_eq!(p.time(), 1.0);
    // row 16 at 120 BPM = 2 s: the second note fires after the seek point
    f32_blocks(&mut p, (1.1 * 44100.0 / 512.0) as usize);
    assert!(p.instruments()[0].voices().iter().any(|v| v.held));
    // mute: no new notes, but a sounding tail keeps rendering
    let mut p = player(b, 0.0);
    p.start(0.0);
    f32_blocks(&mut p, 10);
    p.mute(0, true);
    p.instruments()[0].note_off(60);
    let tail = f32_blocks(&mut p, 5);
    assert!(tail.iter().any(|v| *v != 0.0), "release tail");
    f32_blocks(&mut p, 400);
    assert!(p.instruments()[0].voices().iter().all(|v| !v.active()), "the muted note at row 16 never started");
    p.stop();
    let mut out = [1i16; 2 * BLOCK];
    let t = p.time();
    p.render_i16(&mut out);
    assert!(out.iter().all(|v| *v == 0) && p.time() == t);
}

#[test]
fn i16_uses_the_reference_mix() {
    let b = leak(tfm(120, &[(lead(), LEAD.to_vec()), (bass(), vec![(0, 36, 120), (8, 36, 0)])]));
    let (mut a, mut c) = (player(b, 0.0), player(b, 0.0));
    a.start(0.0);
    c.start(0.0);
    let (mut fo, mut io) = ([0.0f32; 2 * BLOCK], [0i16; 2 * BLOCK]);
    for _ in 0..30 {
        a.render_f32(&mut fo);
        c.render_i16(&mut io);
        for k in 0..2 * BLOCK {
            // one instrument at a time is not separable from the f32 sum, so check the bound: |i16| ≈ f32·11·200
            let want = (fo[k] * 11.0 * 200.0).clamp(-32768.0, 32767.0);
            assert!((io[k] as f32 - want).abs() <= 1.0 + want.abs() * 1e-5, "{k}: {} vs {want}", io[k]);
        }
    }
}

/// Spec "Same timing on a shared track": a one-instrument `.tfm` and a `synth-mix` song with the kind-32 device
/// and the same notes (each placed inside the block the `.tfm` window assigns it to) are bit-identical before the
/// player's output scaling.
#[test]
fn tfm_and_shared_track_are_bit_identical() {
    let tempo = 133u16; // non-integral frames per row
    let rows: Vec<(u16, u8, u8)> = vec![(0, 60, 100), (3, 64, 90), (2, 60, 0), (1, 67, 110), (5, 64, 0), (4, 67, 0)];
    let b = leak(tfm(tempo, &[(lead(), rows.clone())]));
    let mut p = player(b, 0.0);
    p.start(0.0);
    let blocks = p.blocks() + 40;
    let want = f32_blocks(&mut p, blocks);

    let spr = 1.0 / ((tempo as u32 * 4) as f32 / 60.0);
    let (mut row, mut prev) = (0u32, 0u32);
    let mut lane = vec![];
    for (i, &(d, n, v)) in rows.iter().enumerate() {
        row += d as u32;
        let frame = (block_of(row as f32 * spr) * BLOCK + (i * 97) % BLOCK) as u32; // anywhere in that block
        lane.push(((frame - prev) as i32, n, if v == 0 { None } else { Some(v) }));
        prev = frame;
    }
    let blob = leak(ws_songs::song(blocks as f64 * 512.0 / 44100.0, &[(kind::INSTRUMENT, ws_songs::chunk(&lead()))], &[lane], &[ws_songs::Track::new(&[0], 0)]));
    let s = Song::parse(blob).unwrap();
    let env = Env::for_song(&s, None);
    let mem: &'static mut [u8] = Box::leak(vec![0u8; Player::<TfDevice, WsRng>::required_pool(&s, &env, DEFAULT_SUB).unwrap()].into_boxed_slice());
    let mut pool = Pool::new(mem);
    let mut mp = Player::<TfDevice, WsRng>::new(s, &env, &mut pool, WsRng::new(), DEFAULT_SUB).unwrap();
    let mut got = vec![0.0f32; 2 * mp.length_frames() as usize];
    mp.render_f32(&mut got);
    let n = got.len().min(want.len());
    assert!(n > 2 * 40 * BLOCK);
    assert!(got[..n] == want[..n]);
}
