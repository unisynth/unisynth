//! Shared helpers: a song writer and a "loads and renders in the unified player" check.
#![allow(dead_code)]

pub mod metrics;
pub use synth_convert::model::SongM;
use synth_convert::model::{Auto, Dev, Trk};
use synth_core::mix::{Env, Player, Song};
use synth_core::pool::Pool;
use unisynth::device::UDevice;
use unisynth::rng::Xorshift;

pub fn dev(kind: u8, params: &[f32]) -> Dev {
    let mut chunk: Vec<u8> = params.iter().flat_map(|p| p.to_le_bytes()).collect();
    chunk.extend((chunk.len() as i32 + 4).to_le_bytes()); // upstream's trailing size
    Dev { kind, chunk }
}

pub fn track(devices: &[usize], lane: usize) -> Trk {
    Trk { volume: 1.0, receives: vec![], devices: devices.to_vec(), lane, autos: vec![] }
}

pub fn auto(device: usize, param: usize, points: &[(i32, u8)]) -> Auto {
    Auto { device, param, points: points.to_vec() }
}

pub fn song(devices: Vec<Dev>, lanes: Vec<Vec<(i32, u8, Option<u8>)>>, tracks: Vec<Trk>) -> SongM {
    SongM { tempo: 125, rate: 44100, length: 2.0, devices, lanes, tracks }
}

/// Loads `blob` into the unified player (no legacy kind may be left) and renders half a second.
pub fn loads_and_renders(blob: &[u8]) -> Vec<f32> {
    let s = Song::parse(blob).expect("a valid song");
    let env = Env::for_song(&s, None);
    let need = Player::<UDevice, Xorshift>::required_pool(&s, &env, 100).expect("every kind is unified");
    let mut mem = vec![0u8; need];
    let mut pool = Pool::new(&mut mem);
    let mut p: Player<UDevice, Xorshift> = Player::new(s, &env, &mut pool, Xorshift::new(1), 100).expect("loads");
    let mut out = vec![0.0f32; 2 * 22050];
    p.render_f32(&mut out);
    assert!(out.iter().all(|v| v.is_finite()));
    out
}
