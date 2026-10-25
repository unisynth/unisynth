//! Renders a WaveSabre song blob to a 16-bit stereo WAV with the Rust port (single-threaded, 100-frame steps).
//! `cargo run --release -p wavesabre --example render -- song.bin out.wav [gm.dls]`

use synth_core::mix::{Env, Player, Song, DEFAULT_SUB};
use synth_core::pool::Pool;
use wavesabre::helpers::WsRng;
use wavesabre::WsDevice;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 {
        eprintln!("usage: render <song.bin> <out.wav> [bank.dls]");
        std::process::exit(2);
    }
    let blob = std::fs::read(&args[1]).expect("read song");
    let bank = args.get(3).map(|p| std::fs::read(p).expect("read bank"));
    let song = Song::parse(&blob).unwrap_or_else(|e| panic!("bad song: {e:?}"));
    let env = Env::for_song(&song, bank.as_deref());
    let need = Player::<WsDevice, WsRng>::required_pool(&song, &env, DEFAULT_SUB).unwrap_or_else(|e| panic!("{e:?}"));
    let mut mem = vec![0u8; need];
    let mut pool = Pool::new(&mut mem);
    let t0 = std::time::Instant::now();
    let mut p = Player::<WsDevice, WsRng>::new(song, &env, &mut pool, WsRng::new(), DEFAULT_SUB)
        .unwrap_or_else(|e| panic!("{e:?}"));
    let frames = p.length_frames() as usize;
    let mut pcm = vec![0i16; frames * 2];
    p.render_i16(&mut pcm);
    let secs = t0.elapsed().as_secs_f64();
    let peak = pcm.iter().map(|s| s.unsigned_abs()).max().unwrap_or(0);
    let len = frames as f64 / 44100.0;
    eprintln!("{}: {len:.1}s rendered in {secs:.2}s ({:.1}x realtime), peak {peak}, pool {need} B", args[1], len / secs);
    let data = (pcm.len() * 2) as u32;
    let mut w = Vec::with_capacity(44 + data as usize);
    w.extend(b"RIFF");
    w.extend((36 + data).to_le_bytes());
    w.extend(b"WAVEfmt ");
    w.extend(16u32.to_le_bytes());
    w.extend(1u16.to_le_bytes());
    w.extend(2u16.to_le_bytes());
    w.extend(44100u32.to_le_bytes());
    w.extend((44100u32 * 4).to_le_bytes());
    w.extend(4u16.to_le_bytes());
    w.extend(16u16.to_le_bytes());
    w.extend(b"data");
    w.extend(data.to_le_bytes());
    w.extend(pcm.iter().flat_map(|s| s.to_le_bytes()));
    std::fs::write(&args[2], w).expect("write wav");
}
