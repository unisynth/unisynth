//! Loads a unified song with whatever modules this build has, renders it and reports the peak. Exits 1 with the missing
//! module's name when the build lacks one, so `tools/check-song-features.sh` can prove a feature list is enough.
//!   cargo run --release -p unisynth --no-default-features --features std,<features> --example render -- song.bin [out.wav] [gm.dls]

use synth_core::mix::{Env, LoadError, Player, Song, DEFAULT_SUB};
use synth_core::pool::Pool;
use unisynth::device::UDevice;
use unisynth::module::name;
use unisynth::rng::Xorshift;

fn fail(e: LoadError) -> ! {
    match e {
        LoadError::UnsupportedModule(id) => eprintln!("render: this build lacks the {} (module {id})", name(id)),
        e => eprintln!("render: {e:?}"),
    }
    std::process::exit(1)
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("usage: render <song.bin> [out.wav] [bank.dls]");
        std::process::exit(2);
    }
    let blob = std::fs::read(&args[1]).expect("read song");
    let bank = args.get(3).map(|p| std::fs::read(p).expect("read bank"));
    // UNISYNTH_SEED varies the random draws (phases, noise): parity checks use it to see how much a number depends on them
    let seed: u32 = std::env::var("UNISYNTH_SEED").ok().and_then(|s| s.parse().ok()).unwrap_or(1);
    let song = Song::parse(&blob).unwrap_or_else(|e| fail(e));
    let env = Env::for_song(&song, bank.as_deref());
    let need = Player::<UDevice, Xorshift>::required_pool(&song, &env, DEFAULT_SUB).unwrap_or_else(|e| fail(e));
    let mut mem = vec![0u8; need];
    let mut pool = Pool::new(&mut mem);
    let mut p = Player::<UDevice, Xorshift>::new(song, &env, &mut pool, Xorshift::new(seed), DEFAULT_SUB).unwrap_or_else(|e| fail(e));
    // US_NO_SKIP=1: render every track every sub-block (A/B of the quiet-track skip)
    p.set_skip_quiet(std::env::var_os("US_NO_SKIP").is_none());
    let frames = p.length_frames() as usize;
    let mut pcm = vec![0i16; frames * 2];
    let t0 = std::time::Instant::now();
    p.render_i16(&mut pcm);
    let secs = t0.elapsed().as_secs_f64();
    let peak = pcm.iter().map(|s| s.unsigned_abs()).max().unwrap_or(0);
    eprintln!("{}: {:.1} s, peak {peak}, pool {need} B, rendered in {secs:.4} s", args[1], frames as f64 / 44100.0);
    if let Some(out) = args.get(2) {
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
        std::fs::write(out, w).expect("write wav");
    }
    if peak == 0 {
        eprintln!("render: the song is silent");
        std::process::exit(3);
    }
}
