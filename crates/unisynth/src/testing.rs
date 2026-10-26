//! Test helpers: a `synth-mix` song blob writer and a player harness (tests only).

use crate::device::UDevice;
use crate::rng::Xorshift;
use synth_core::mix::{Env, Player, Song};
use synth_core::pool::Pool;

/// One track: volume, receives `(track, channel, volume)`, device indices, lane, automations
/// `(device within the track, param, [(delta, value byte)])`.
pub struct Track {
    pub volume: f32,
    pub recv: Vec<(i32, i32, f32)>,
    pub devices: Vec<i32>,
    pub lane: i32,
    pub autos: Vec<(i32, i32, Vec<(i32, u8)>)>,
}

impl Track {
    pub fn new(devices: &[i32], lane: i32) -> Self {
        Track { volume: 1.0, recv: vec![], devices: devices.to_vec(), lane, autos: vec![] }
    }
}

/// A lane event: `(delta frames, note, Some(velocity) for a note-on or None for a note-off)`.
pub type LaneEvent = (i32, u8, Option<u8>);

/// Song blob: `devices` are `(kind, chunk)`, the last track is the master.
pub fn song(rate: i32, seconds: f64, devices: &[(u8, Vec<u8>)], lanes: &[Vec<LaneEvent>], tracks: &[Track]) -> Vec<u8> {
    let mut b = vec![];
    let i = |b: &mut Vec<u8>, v: i32| b.extend(v.to_le_bytes());
    let f = |b: &mut Vec<u8>, v: f32| b.extend(v.to_le_bytes());
    i(&mut b, 120);
    i(&mut b, rate);
    b.extend(seconds.to_le_bytes());
    i(&mut b, devices.len() as i32);
    for (k, c) in devices {
        b.push(*k);
        i(&mut b, c.len() as i32);
        b.extend(c);
    }
    i(&mut b, lanes.len() as i32);
    for l in lanes {
        i(&mut b, l.len() as i32);
        for &(d, n, v) in l {
            i(&mut b, d);
            match v {
                Some(v) => b.extend([n, v]),
                None => b.push(n | 0x80),
            }
        }
    }
    i(&mut b, tracks.len() as i32);
    for t in tracks {
        f(&mut b, t.volume);
        i(&mut b, t.recv.len() as i32);
        for &(s, c, v) in &t.recv {
            i(&mut b, s);
            i(&mut b, c);
            f(&mut b, v);
        }
        i(&mut b, t.devices.len() as i32);
        for &d in &t.devices {
            i(&mut b, d);
        }
        i(&mut b, t.lane);
        i(&mut b, t.autos.len() as i32);
        for (d, p, pts) in &t.autos {
            i(&mut b, *d);
            i(&mut b, *p);
            i(&mut b, pts.len() as i32);
            for &(dt, v) in pts {
                i(&mut b, dt);
                b.push(v);
            }
        }
    }
    b
}

/// Loads `bytes` into a player with the given sub-block size and runs `f`.
pub fn with_player<R>(bytes: &[u8], sub: usize, f: impl for<'x> FnOnce(&mut Player<'x, UDevice<'x>, Xorshift>) -> R) -> R {
    let s = Song::parse(bytes).unwrap();
    let env = Env::for_song(&s, None);
    let need = Player::<UDevice, Xorshift>::required_pool(&s, &env, sub).unwrap();
    let mut mem = vec![0u8; need];
    let mut pool = Pool::new(&mut mem);
    let mut p = Player::new(s, &env, &mut pool, Xorshift::new(99), sub).unwrap();
    f(&mut p)
}

/// Renders `frames` interleaved stereo frames in calls of `call` frames.
pub fn render<'a>(p: &mut Player<'a, UDevice<'a>, Xorshift>, frames: usize, call: usize) -> Vec<f32> {
    let mut out = vec![];
    let mut left = frames;
    while left > 0 {
        let n = call.min(left);
        let mut buf = vec![0.0f32; n * 2];
        p.render_f32(&mut buf);
        out.extend(buf);
        left -= n;
    }
    out
}
