//! Renders the embedded song and prints FNV-1a hashes of the output: f32 from frame 0 and i16 after seeks (one inside
//! a sub-block), for sub-block sizes 100 and 77. `check.sh` builds it from the blob and from the generated Rust (with
//! `us-trusted`) and requires the same lines.

use synth_core::mix::{Env, Player, Song};
use synth_core::pool::Pool;
use unisynth::device::UDevice;
use unisynth::rng::Xorshift;

#[cfg(feature = "baked")]
mod baked {
    include!(concat!(env!("OUT_DIR"), "/song.rs"));
}

fn song() -> Song<'static> {
    #[cfg(feature = "baked")]
    return baked::SONG;
    #[cfg(not(feature = "baked"))]
    Song::parse(include_bytes!(concat!(env!("OUT_DIR"), "/song.bin"))).expect("a valid song")
}

fn fnv(bytes: impl Iterator<Item = u8>) -> u64 {
    bytes.fold(0xcbf2_9ce4_8422_2325, |h, b| (h ^ b as u64).wrapping_mul(0x100_0000_01b3))
}

fn main() {
    for sub in [100, 77] {
        let s = song();
        let env = Env::for_song(&s, None);
        let mut mem = vec![0u8; Player::<UDevice, Xorshift>::required_pool(&s, &env, sub).expect("pool")];
        let mut pool = Pool::new(&mut mem);
        let mut p = Player::<UDevice, Xorshift>::new(s, &env, &mut pool, Xorshift::new(1), sub).expect("loads");
        let n = (p.length_frames() as usize).min(44_100 * 20);
        let mut f = vec![0.0f32; 2 * n];
        p.render_f32(&mut f);
        let mut lines = vec![format!("sub {sub} f32 {:016x}", fnv(f.iter().flat_map(|x| x.to_le_bytes())))];
        for at in [44_100 * 3 + 37, 1_000] {
            p.seek(at);
            let mut i = vec![0i16; 2 * 44_100 * 2];
            p.render_i16(&mut i);
            lines.push(format!("sub {sub} seek {at} i16 {:016x}", fnv(i.iter().flat_map(|x| x.to_le_bytes()))));
        }
        lines.iter().for_each(|l| println!("{l}"));
    }
}
