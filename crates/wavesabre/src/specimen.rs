//! Upstream `Specimen` (kind 12): a GSM 6.10 sample in the chunk, played by the shared sample voice. Chunk:
//! `{compressed i32, uncompressed i32}`, WAVEFORMATEX (18 + cbSize), GSM blocks, f32 params, trailing size.

use crate::gsm::GsmChunk;
use crate::sampler::{sampler_device, Sampler, Source, P};
use crate::synth::CommonParam as C;
use synth_core::mix::{Env, LoadError};
use synth_core::pool::Pool;
use synth_core::rng::Rng;

const MAP: [P; 26] = [
    P::Amp(0), P::Amp(1), P::Amp(2), P::Amp(3), P::SampleStart, P::LoopMode, P::LoopStart, P::LoopLength,
    P::Reverse, P::Interp, P::Coarse, P::Fine, P::FilterType, P::FilterFreq, P::FilterRes, P::FilterMod,
    P::Mod(0), P::Mod(1), P::Mod(2), P::Mod(3), P::Common(C::Unisono), P::Common(C::Detune), P::Common(C::Pan),
    P::Master, P::Common(C::VoiceMode), P::Common(C::Slide),
];

pub struct Specimen<'a> {
    s: Sampler<'a>,
}

impl<'a> Specimen<'a> {
    pub fn need(chunk: &[u8], _: &Env) -> Result<usize, LoadError> {
        Ok(Sampler::need() + GsmChunk::pool_need(chunk)?)
    }

    pub fn new(chunk: &'a [u8], _: &Env, pool: &mut Pool<'a>, _: &mut dyn Rng) -> Result<Self, LoadError> {
        let data = match GsmChunk::parse(chunk)? {
            Some(g) => g.decode(pool)?,
            None => &[],
        };
        Ok(Specimen { s: Sampler::new(pool, Source::F32(data), false)? })
    }

    /// Params start after the compressed data (`(size − 4 − offset) / 4` floats, upstream `SetChunk`).
    pub fn param_bytes(chunk: &[u8]) -> &[u8] {
        match GsmChunk::parse(chunk) {
            Ok(Some(g)) => g.rest,
            _ => &[],
        }
    }

    fn extra_param(&mut self, _: usize, _: f32) {}
}

sampler_device!(Specimen, MAP);
