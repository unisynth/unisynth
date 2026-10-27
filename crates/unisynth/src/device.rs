//! `UDevice`: the unified synth's device-kind enum (registry for `synth_core::mix`). One variant per kind family. Combine with another engine's registry through `synth_core::mix::Either`.

use crate::fx::{is_effect, UFx};
use crate::instrument::UInstrument;
use crate::kind;
use synth_core::device::{Ctx, Device, Io};
use synth_core::mix::{Env, Kinds, LoadError};
use synth_core::pool::Pool;
use synth_core::rng::Rng;

pub enum UDevice<'a> {
    Instrument(UInstrument<'a>),
    Effect(UFx<'a>),
}

impl<'a> Kinds<'a> for UDevice<'a> {
    /// Instruments and effects draw only from their own generators while rendering (seeded at load).
    #[cfg(feature = "threads")]
    const RENDER_RNG: bool = false;

    fn need(k: u8, chunk: &'a [u8], env: &Env<'a>) -> Result<usize, LoadError> {
        match k {
            kind::INSTRUMENT => UInstrument::need(chunk),
            k if is_effect(k) => UFx::need(k, chunk, env),
            k => Err(LoadError::UnsupportedKind(k)),
        }
    }

    fn load(k: u8, chunk: &'a [u8], env: &Env<'a>, pool: &mut Pool<'a>, rng: &mut dyn Rng) -> Result<Self, LoadError> {
        match k {
            kind::INSTRUMENT => Ok(UDevice::Instrument(UInstrument::new(chunk, env, pool, rng)?)),
            k if is_effect(k) => Ok(UDevice::Effect(UFx::new(k, chunk, env, pool)?)),
            k => Err(LoadError::UnsupportedKind(k)),
        }
    }
}

macro_rules! forward {
    ($self:ident, $d:ident => $e:expr) => {
        match $self {
            UDevice::Instrument($d) => $e,
            UDevice::Effect($d) => $e,
        }
    };
}

impl Device for UDevice<'_> {
    fn param_count(&self) -> usize {
        forward!(self, d => d.param_count())
    }
    fn set_param(&mut self, index: usize, value: f32) {
        forward!(self, d => d.set_param(index, value))
    }
    fn note_on(&mut self, note: u8, velocity: u8, offset: u32) {
        forward!(self, d => d.note_on(note, velocity, offset))
    }
    fn note_off(&mut self, note: u8, offset: u32) {
        forward!(self, d => d.note_off(note, offset))
    }
    fn all_notes_off(&mut self) {
        forward!(self, d => d.all_notes_off())
    }
    fn event_lookahead(&self) -> u32 {
        forward!(self, d => d.event_lookahead())
    }
    fn reset(&mut self) {
        forward!(self, d => d.reset())
    }
    fn run(&mut self, io: &mut Io, n: usize, ctx: &mut Ctx) {
        forward!(self, d => d.run(io, n, ctx))
    }
}
