//! Device kind registry. `synth-core` cannot know the engines (they depend on it), so each engine — or a facade for
//! mixed songs — supplies an enum of its enabled device kinds implementing [`Kinds`] + `Device`. Dispatch is then a
//! `match`, with no per-device vtables, and disabled cargo features simply have no enum variant.

use super::{LoadError, Song};
use crate::device::Device;
use crate::pool::Pool;
use crate::rng::Rng;

/// Load-time context for building devices.
#[derive(Clone, Copy, Debug)]
pub struct Env<'a> {
    pub rate: f64,
    pub tempo: i32,
    /// Optional DLS sound bank bytes (GM.DLS sampler); devices that need one play silence without it.
    pub bank: Option<&'a [u8]>,
}

impl<'a> Env<'a> {
    pub fn for_song(song: &Song<'a>, bank: Option<&'a [u8]>) -> Self {
        Env { rate: song.rate as f64, tempo: song.tempo, bank }
    }
}

pub trait Kinds<'a>: Device + Sized {
    /// Pool bytes `load` needs for this device (upper bound). Unknown or disabled kind → `UnsupportedKind`.
    fn need(kind: u8, chunk: &'a [u8], env: &Env<'a>) -> Result<usize, LoadError>;
    /// Builds the device from its chunk, carving its state from `pool`. Load-time random draws (upstream voice
    /// constructors) come from the song random source `rng`, in song device order.
    fn load(kind: u8, chunk: &'a [u8], env: &Env<'a>, pool: &mut Pool<'a>, rng: &mut dyn Rng)
        -> Result<Self, LoadError>;
}

/// Pool bytes [`load_devices`] needs for `song`.
pub fn devices_need<'a, K: Kinds<'a>>(song: &Song<'a>, env: &Env<'a>) -> Result<usize, LoadError> {
    let mut sum = Pool::need::<K>(song.device_count());
    for d in song.devices() {
        sum += K::need(d.kind, d.chunk, env)?;
    }
    Ok(sum)
}

/// Builds every device of `song`, in song order. Kinds are checked before anything is built.
pub fn load_devices<'a, K: Kinds<'a>>(
    song: &Song<'a>,
    env: &Env<'a>,
    pool: &mut Pool<'a>,
    rng: &mut dyn Rng,
) -> Result<&'a mut [K], LoadError> {
    for d in song.devices() {
        K::need(d.kind, d.chunk, env)?;
    }
    let slots = pool.uninit::<K>(song.device_count())?;
    let mut built = 0;
    for (slot, d) in slots.iter_mut().zip(song.devices()) {
        slot.write(K::load(d.kind, d.chunk, env, pool, rng)?);
        built += 1;
    }
    if built != slots.len() {
        return Err(LoadError::Malformed);
    }
    // SAFETY: all `slots.len()` slots were written above; MaybeUninit<K> has K's layout.
    Ok(unsafe { &mut *(slots as *mut [core::mem::MaybeUninit<K>] as *mut [K]) })
}

/// Facade for songs that mix engines: tries `A`'s kinds first and falls back to `B`'s when `A` reports
/// `UnsupportedKind` (e.g. `Either<WsDevice, TfDevice>` holds WaveSabre kinds 0–12 and Tunefish kinds 32–39).
pub enum Either<A, B> {
    A(A),
    B(B),
}

impl<'a, A: Kinds<'a>, B: Kinds<'a>> Kinds<'a> for Either<A, B> {
    fn need(kind: u8, chunk: &'a [u8], env: &Env<'a>) -> Result<usize, LoadError> {
        match A::need(kind, chunk, env) {
            Err(LoadError::UnsupportedKind(_)) => B::need(kind, chunk, env),
            r => r,
        }
    }

    fn load(kind: u8, chunk: &'a [u8], env: &Env<'a>, pool: &mut Pool<'a>, rng: &mut dyn Rng)
        -> Result<Self, LoadError> {
        match A::need(kind, chunk, env) {
            Err(LoadError::UnsupportedKind(_)) => B::load(kind, chunk, env, pool, rng).map(Either::B),
            _ => A::load(kind, chunk, env, pool, rng).map(Either::A),
        }
    }
}

macro_rules! forward {
    ($self:ident, $d:ident => $e:expr) => {
        match $self {
            Either::A($d) => $e,
            Either::B($d) => $e,
        }
    };
}

impl<A: Device, B: Device> Device for Either<A, B> {
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
    fn run(&mut self, io: &mut crate::device::Io, n: usize, ctx: &mut crate::device::Ctx) {
        forward!(self, d => d.run(io, n, ctx))
    }
}
