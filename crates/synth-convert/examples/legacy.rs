//! Renders a legacy synth-mix song (WaveSabre kinds 0–12 or Tunefish kinds 32–39) with its legacy crate, to a 16-bit
//! stereo 44.1 kHz WAV, for `tools/unisynth-parity`. Prints `engine seconds-of-audio render-seconds` on stdout.
//! `cargo run --release -p synth-convert --example legacy -- <song.bin> <out.wav> [bank.dls]`
//! A `.tfm` file plays through the reference Tunefish player: `legacy <song.tfm> <out.wav> [tail-seconds]`.

use synth_core::mix::{Env, Player, Song, DEFAULT_SUB};
use synth_core::pool::Pool;
use tfsynth::device::TfDevice;
use tfsynth::player::TfmPlayer;
use tfsynth::tfm::Tfm;
use tfsynth::voice::BLOCK;
use wavesabre::helpers::WsRng;
use wavesabre::WsDevice;

macro_rules! render {
    ($dev:ty, $song:expr, $bank:expr) => {{
        let song = $song;
        let env = Env::for_song(&song, $bank);
        let need = Player::<$dev, WsRng>::required_pool(&song, &env, DEFAULT_SUB).unwrap_or_else(|e| fail(e));
        let mut mem = vec![0u8; need];
        let mut pool = Pool::new(&mut mem);
        let mut p = Player::<$dev, WsRng>::new(song, &env, &mut pool, WsRng::new(), DEFAULT_SUB).unwrap_or_else(|e| fail(e));
        let mut pcm = vec![0i16; 2 * p.length_frames() as usize];
        let t0 = std::time::Instant::now();
        p.render_i16(&mut pcm);
        (pcm, t0.elapsed().as_secs_f64())
    }};
}

fn fail(e: impl std::fmt::Debug) -> ! {
    eprintln!("legacy: {e:?}");
    std::process::exit(1)
}

fn write(path: &str, pcm: &[i16]) {
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
    std::fs::write(path, w).expect("write wav");
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    if a.len() < 3 {
        eprintln!("usage: legacy <song.bin> <out.wav> [bank.dls]");
        std::process::exit(2);
    }
    let blob = std::fs::read(&a[1]).expect("read song");
    if a[1].ends_with(".tfm") {
        let tail: f32 = a.get(3).map_or(0.0, |t| t.parse().expect("tail seconds"));
        let song = Tfm::parse(&blob).unwrap_or_else(|e| fail(e));
        let mut mem = vec![0u8; TfmPlayer::required_pool(&song)];
        let mut pool = Pool::new(&mut mem);
        let mut p = TfmPlayer::new(song, &mut pool, 0.0).expect("pool");
        let blocks = p.blocks() + (tail * 44100.0 / BLOCK as f32) as usize;
        p.start(0.0);
        let mut pcm = Vec::with_capacity(blocks * 2 * BLOCK);
        let mut out = [0i16; 2 * BLOCK];
        let t0 = std::time::Instant::now();
        for _ in 0..blocks {
            p.render_i16(&mut out);
            pcm.extend_from_slice(&out);
        }
        write(&a[2], &pcm);
        println!("tfsynth {:.3} {:.4}", pcm.len() as f64 / 2.0 / 44100.0, t0.elapsed().as_secs_f64());
        return;
    }
    let bank = a.get(3).map(|p| std::fs::read(p).expect("read bank"));
    let song = Song::parse(&blob).unwrap_or_else(|e| fail(e));
    let tf = song.devices().any(|d| (32..=39).contains(&d.kind));
    let (pcm, secs) = if tf { render!(TfDevice, song, bank.as_deref()) } else { render!(WsDevice, song, bank.as_deref()) };
    write(&a[2], &pcm);
    println!("{} {:.3} {:.4}", if tf { "tfsynth" } else { "wavesabre" }, pcm.len() as f64 / 2.0 / 44100.0, secs);
}
