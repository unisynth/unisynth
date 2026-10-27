//! Renders a song blob through `synth_core::mix::Player` with the unified devices — the same code the intro plays.
//! The song random source is `Xorshift::new(SEED)`; the live host's bounce uses this function too.

use synth_core::mix::{Env, Player, Song, DEFAULT_SUB};
use synth_core::pool::Pool;
use unisynth::rng::Xorshift;
use unisynth::UDevice;

/// Seed of the song random source (device phases, noise seeds).
pub const SEED: u32 = 1;

/// Interleaved stereo f32 of the whole song (`length · rate` frames). `bank`: GM.DLS bytes for sampler slots.
pub fn render(blob: &[u8], bank: Option<&[u8]>) -> Result<Vec<f32>, String> {
    let s = Song::parse(blob).map_err(|e| format!("not a valid song blob: {e:?}"))?;
    let env = Env::for_song(&s, bank);
    let need = Player::<UDevice, Xorshift>::required_pool(&s, &env, DEFAULT_SUB).map_err(|e| format!("cannot load: {e:?}"))?;
    let mut mem = vec![0u8; need];
    let mut pool = Pool::new(&mut mem);
    let mut p = Player::<UDevice, Xorshift>::new(s, &env, &mut pool, Xorshift::new(SEED), DEFAULT_SUB).map_err(|e| format!("cannot load: {e:?}"))?;
    let mut out = vec![0.0f32; p.length_frames() as usize * 2];
    p.render_f32(&mut out);
    Ok(out)
}

/// [`render`] with `threads ≥ 1` workers (the calling thread plus `threads − 1` helpers): bit-identical output;
/// 1 = the single-thread path.
#[cfg(feature = "threads")]
pub fn render_threads(blob: &[u8], bank: Option<&[u8]>, threads: usize) -> Result<Vec<f32>, String> {
    if threads <= 1 {
        return render(blob, bank);
    }
    let s = Song::parse(blob).map_err(|e| format!("not a valid song blob: {e:?}"))?;
    let env = Env::for_song(&s, bank);
    let need = Player::<UDevice, Xorshift>::required_pool(&s, &env, DEFAULT_SUB).map_err(|e| format!("cannot load: {e:?}"))?;
    let mut mem = vec![0u8; need];
    let mut pool = Pool::new(&mut mem);
    let mut p = Player::<UDevice, Xorshift>::new(s, &env, &mut pool, Xorshift::new(SEED), DEFAULT_SUB).map_err(|e| format!("cannot load: {e:?}"))?;
    let mut out = vec![0.0f32; p.length_frames() as usize * 2];
    crate::workers::with_workers(threads, |w| p.render_f32_par(&mut out, w));
    Ok(out)
}

/// A WAV file of interleaved stereo samples: 16-bit PCM (`clamp(trunc(x·32767))`, the player's conversion) or
/// 32-bit float.
pub fn wav(samples: &[f32], rate: u32, float: bool) -> Vec<u8> {
    let bytes = if float { 4 } else { 2 };
    let data = (samples.len() * bytes) as u32;
    let mut w = Vec::with_capacity(44 + data as usize);
    w.extend(b"RIFF");
    w.extend((36 + data).to_le_bytes());
    w.extend(b"WAVEfmt ");
    w.extend(16u32.to_le_bytes());
    w.extend((if float { 3u16 } else { 1u16 }).to_le_bytes());
    w.extend(2u16.to_le_bytes());
    w.extend(rate.to_le_bytes());
    w.extend((rate * 2 * bytes as u32).to_le_bytes());
    w.extend((2 * bytes as u16).to_le_bytes());
    w.extend((8 * bytes as u16).to_le_bytes());
    w.extend(b"data");
    w.extend(data.to_le_bytes());
    for &x in samples {
        if float {
            w.extend(x.to_le_bytes());
        } else {
            w.extend((((x * 32767.0) as i32).clamp(-32768, 32767) as i16).to_le_bytes());
        }
    }
    w
}
