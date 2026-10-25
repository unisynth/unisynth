//! Upstream `Adultery` (kind 11): plays wave `SampleIndex − 1` of the GM.DLS bank through the shared sample voice.
//! The wave is read in place from `Env::bank` via `crates/dls` (pool-table order, which equals upstream's file-order
//! walk of the Windows bank from 0x44602 — asserted in tests); loop = the wave's `wsmp` loop, else the whole wave.
//! No bank, an unparsable bank or an out-of-range index → silence (upstream would hang, or read past the pool).

use crate::sampler::{sampler_device, Sampler, Source, P};
use crate::synth::CommonParam as C;
use synth_core::mix::{Env, LoadError};
use synth_core::pool::Pool;
use synth_core::rng::Rng;

const MAP: [P; 28] = [
    P::Extra, P::Amp(0), P::Amp(1), P::Amp(2), P::Amp(3), P::SampleStart, P::LoopMode, P::LoopBoundary,
    P::LoopStart, P::LoopLength, P::Reverse, P::Interp, P::Coarse, P::Fine, P::FilterType, P::FilterFreq,
    P::FilterRes, P::FilterMod, P::Mod(0), P::Mod(1), P::Mod(2), P::Mod(3), P::Common(C::Unisono),
    P::Common(C::Detune), P::Common(C::Pan), P::Master, P::Common(C::VoiceMode), P::Common(C::Slide),
];

pub struct Adultery<'a> {
    s: Sampler<'a>,
    bank: Option<dls::Dls<'a>>,
}

impl<'a> Adultery<'a> {
    pub fn need(_: &[u8], _: &Env) -> Result<usize, LoadError> {
        Ok(Sampler::need())
    }

    pub fn new(_: &[u8], env: &Env<'a>, pool: &mut Pool<'a>, _: &mut dyn Rng) -> Result<Self, LoadError> {
        Ok(Adultery {
            s: Sampler::new(pool, Source::F32(&[]), true)?,
            bank: env.bank.and_then(|b| dls::Dls::parse(b).ok()),
        })
    }

    /// `SampleIndex`: `(int)v − 1`; voices already sounding keep their wave.
    fn extra_param(&mut self, _: usize, v: f32) {
        let index = v as i32 - 1;
        let wave = if index >= 0 { self.bank.and_then(|b| b.wave(index as u32)) } else { None };
        let p = &mut self.s.p;
        match wave {
            Some(w) => {
                p.src = Source::Wave(w);
                let len = p.src.len();
                p.sample_loop = match w.wsmp.looped {
                    Some((s, l)) => (s.min(i32::MAX as u32) as i32, l.min(i32::MAX as u32) as i32),
                    None => (0, len),
                };
            }
            None => {
                p.src = Source::F32(&[]);
                p.sample_loop = (0, 0);
            }
        }
    }
}

sampler_device!(Adultery, MAP);
