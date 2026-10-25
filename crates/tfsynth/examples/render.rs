//! Renders with `tfsynth` to a 16-bit stereo WAV.
//!   cargo run --release -p tfsynth --example render -- <song.tfm> <out.wav> [tail-seconds]   (reference player mix)
//!   cargo run --release -p tfsynth --example render -- note <out.wav> [midi-note]   (demo patch, 2 s note + 1 s tail)

use synth_core::pool::Pool;
use tfsynth::instrument::Instrument;
use tfsynth::params::*;
use tfsynth::player::TfmPlayer;
use tfsynth::tfm::Tfm;
use tfsynth::voice::BLOCK;

fn wav(path: &str, pcm: &[i16]) {
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

/// A small demo patch: 12 partials with some width, 3 detuned layers, ADSR1 → volume, LP sweep by ADSR2.
fn demo() -> [f32; COUNT] {
    let mut p = [0.0f32; COUNT];
    p[GAIN] = 0.5;
    p[GEN_BANDWIDTH] = 0.1;
    p[GEN_HARMONICS] = 12.0 / 64.0;
    p[GEN_DAMP] = 0.3;
    p[GEN_MOD] = 0.4;
    p[GEN_VOLUME] = 1.0;
    p[GEN_PAN] = 0.5;
    p[GEN_OCTAVE] = 0.5;
    p[GEN_POLY] = 1.0;
    p[GEN_UNISON] = 2.0 / 9.0;
    p[GEN_SPREAD] = 0.4;
    p[GEN_SCALE] = 0.25;
    p[NOISE_BW] = 1.0;
    p[LP_ON] = 1.0;
    p[LP_CUT] = 0.6;
    p[LP_RES] = 0.3;
    p[ENV1..ENV1 + 5].copy_from_slice(&[0.2, 0.6, 0.6, 0.6, 0.5]);
    p[ENV2..ENV2 + 5].copy_from_slice(&[0.1, 0.7, 0.3, 0.6, 0.5]);
    for i in 0..10 {
        p[MM_FIRST + 3 * i + 1] = 0.5;
    }
    p[MM_FIRST..MM_FIRST + 3].copy_from_slice(&[3.0 / 14.0, 0.5, 5.0 / 39.0]); // ADSR1 → volume
    p[MM_FIRST + 3..MM_FIRST + 6].copy_from_slice(&[4.0 / 14.0, 0.5, 12.0 / 39.0]); // ADSR2 → LP cutoff
    p[BEND_UP] = 1.0 / 12.0;
    p[BEND_DOWN] = 1.0 / 12.0;
    p
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("note") if args.len() >= 3 => {
            let note: u8 = args.get(3).map_or(48, |n| n.parse().expect("note"));
            let mut mem = vec![0u8; Instrument::need(0)];
            let mut pool = Pool::new(&mut mem);
            let mut ins = Instrument::new(&mut pool, 0).expect("pool");
            ins.set_patch(&demo()).unwrap();
            ins.note_on(note, 110);
            let mut pcm = vec![];
            let mut out = [[0.0f32; BLOCK]; 2];
            let blocks = 3 * 44100 / BLOCK;
            for b in 0..blocks {
                if b == 2 * 44100 / BLOCK {
                    ins.note_off(note);
                }
                ins.render_voices(&mut out);
                for i in 0..BLOCK {
                    for c in 0..2 {
                        pcm.push((out[c][i] * 11.0 * 200.0).clamp(-32768.0, 32767.0) as i16);
                    }
                }
            }
            wav(&args[2], &pcm);
            let peak = pcm.iter().map(|s| s.unsigned_abs()).max().unwrap_or(0);
            eprintln!("{}: note {note}, {:.2} s, peak {peak}", args[2], pcm.len() as f64 / 88200.0);
        }
        Some(path) if args.len() >= 3 => {
            let bytes = std::fs::read(path).expect("read song");
            let song = Tfm::parse(&bytes).unwrap_or_else(|e| panic!("bad .tfm: {e:?}"));
            let tail: f32 = args.get(3).map_or(0.0, |t| t.parse().expect("tail seconds"));
            let mut mem = vec![0u8; TfmPlayer::required_pool(&song)];
            let mut pool = Pool::new(&mut mem);
            let t0 = std::time::Instant::now();
            let mut p = TfmPlayer::new(song, &mut pool, 0.0).expect("pool");
            let blocks = p.blocks() + (tail * 44100.0 / BLOCK as f32) as usize;
            p.start(0.0);
            let mut pcm = Vec::with_capacity(blocks * 2 * BLOCK);
            let mut out = [0i16; 2 * BLOCK];
            for _ in 0..blocks {
                p.render_i16(&mut out);
                pcm.extend_from_slice(&out);
            }
            let secs = t0.elapsed().as_secs_f64();
            let len = pcm.len() as f64 / 88200.0;
            let peak = pcm.iter().map(|s| s.unsigned_abs()).max().unwrap_or(0);
            wav(&args[2], &pcm);
            eprintln!(
                "{path}: {} instruments, tempo {}, {len:.1} s rendered in {secs:.2} s ({:.1}x realtime), peak {peak}, pool {} B",
                song.instruments(),
                song.tempo,
                len / secs,
                mem.len()
            );
        }
        _ => {
            eprintln!("usage: render <song.tfm> <out.wav> [tail-seconds] | render note <out.wav> [midi-note]");
            std::process::exit(2);
        }
    }
}
