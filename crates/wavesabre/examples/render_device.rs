//! Writes the per-device parity songs (`tests/songs.rs`: every device kind, our own chunks) as `<name>.bin` plus our
//! render `<name>_rust.wav` into `<outdir>`, for `tools/ws-parity` to render the same blobs with upstream.
//! `cargo run --release -p wavesabre --example render_device -- <outdir> [gm.dls]`
//! Without a bank, Adultery songs are skipped (upstream cannot run Adultery without one).

#[path = "../tests/songs.rs"]
#[allow(dead_code)]
mod songs;

use synth_core::mix::{Env, Player, Song, DEFAULT_SUB};
use synth_core::pool::Pool;
use wavesabre::helpers::WsRng;
use wavesabre::WsDevice;

/// 16-bit stereo 44.1 kHz WAV.
fn wav(pcm: &[i16]) -> Vec<u8> {
    let data = (pcm.len() * 2) as u32;
    let mut w = Vec::with_capacity(44 + data as usize);
    w.extend(b"RIFF");
    w.extend((36 + data).to_le_bytes());
    w.extend(b"WAVEfmt ");
    w.extend(16u32.to_le_bytes());
    w.extend(1u16.to_le_bytes()); // PCM
    w.extend(2u16.to_le_bytes()); // channels
    w.extend(44100u32.to_le_bytes());
    w.extend((44100u32 * 4).to_le_bytes());
    w.extend(4u16.to_le_bytes());
    w.extend(16u16.to_le_bytes());
    w.extend(b"data");
    w.extend(data.to_le_bytes());
    w.extend(pcm.iter().flat_map(|s| s.to_le_bytes()));
    w
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let Some(dir) = args.get(1) else {
        eprintln!("usage: render_device <outdir> [bank.dls]");
        std::process::exit(2);
    };
    std::fs::create_dir_all(dir).expect("outdir");
    let bank = args.get(2).map(|p| std::fs::read(p).expect("read bank"));
    let mut list: Vec<_> = songs::all().into_iter().filter(|(n, _)| !n.starts_with("adultery")).collect();
    if bank.is_some() {
        list.extend(songs::bank_songs());
    }
    for (name, blob) in list {
        let song = Song::parse(&blob).expect("song");
        let env = Env::for_song(&song, bank.as_deref());
        let need = Player::<WsDevice, WsRng>::required_pool(&song, &env, DEFAULT_SUB).expect("pool");
        let mut mem = vec![0u8; need];
        let mut pool = Pool::new(&mut mem);
        let mut p = Player::<WsDevice, WsRng>::new(song, &env, &mut pool, WsRng::new(), DEFAULT_SUB).expect("load");
        let mut pcm = vec![0i16; 2 * p.length_frames() as usize];
        p.render_i16(&mut pcm);
        std::fs::write(format!("{dir}/{name}.bin"), &blob).unwrap();
        std::fs::write(format!("{dir}/{name}_rust.wav"), wav(&pcm)).unwrap();
        println!("{name}");
    }
}
