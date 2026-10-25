//! `WsDevice`: the WaveSabre device-kind enum (registry for `synth_core::mix`). One variant per enabled `ws-*`
//! feature; disabled or unknown kinds fail at load with `UnsupportedKind`.

use synth_core::device::{Ctx, Device, Io};
use synth_core::mix::{Env, Kinds, LoadError};
use synth_core::pool::Pool;
use synth_core::rng::Rng;

/// Upstream `DeviceId` values (song blob kind byte).
pub mod kind {
    pub const FALCON: u8 = 0;
    pub const SLAUGHTER: u8 = 1;
    pub const THUNDER: u8 = 2;
    pub const SCISSOR: u8 = 3;
    pub const LEVELLER: u8 = 4;
    pub const CRUSHER: u8 = 5;
    pub const ECHO: u8 = 6;
    pub const SMASHER: u8 = 7;
    pub const CHAMBER: u8 = 8;
    pub const TWISTER: u8 = 9;
    pub const CATHEDRAL: u8 = 10;
    pub const ADULTERY: u8 = 11;
    pub const SPECIMEN: u8 = 12;
}

/// Upstream `Device::SetChunk`: `(size − 4) / 4` little-endian f32 parameters (the trailing i32 is the size).
/// Fewer than `param_count` → the rest keep defaults; more → ignored by `set_param`. Chunks under 4 bytes set nothing
/// (upstream underflows there).
#[allow(dead_code)] // unused only in a build with no devices
pub(crate) fn set_params(dev: &mut impl Device, chunk: &[u8]) {
    let n = chunk.len().saturating_sub(4) / 4;
    for (i, b) in chunk[..n * 4].chunks_exact(4).enumerate() {
        dev.set_param(i, f32::from_le_bytes([b[0], b[1], b[2], b[3]]));
    }
}

/// The part of a chunk holding f32 parameters: after the sample for Specimen, none for Thunder, else all of it.
#[allow(dead_code)]
fn param_bytes(k: u8, chunk: &[u8]) -> &[u8] {
    match k {
        #[cfg(feature = "ws-specimen")]
        kind::SPECIMEN => crate::specimen::Specimen::param_bytes(chunk),
        kind::THUNDER => &[],
        _ => chunk,
    }
}

macro_rules! ws_devices {
    ($( $feat:literal $id:ident $var:ident $ty:ty ),* $(,)?) => {
        pub enum WsDevice<'a> {
            $( #[cfg(feature = $feat)] $var($ty), )*
            #[doc(hidden)]
            _Lifetime(core::marker::PhantomData<&'a ()>, core::convert::Infallible),
        }

        macro_rules! each {
            ($self:expr, $d:ident => $e:expr) => {
                match $self {
                    $( #[cfg(feature = $feat)] WsDevice::$var($d) => $e, )*
                    WsDevice::_Lifetime(_, never) => match *never {},
                }
            };
        }

        impl<'a> Kinds<'a> for WsDevice<'a> {
            fn need(k: u8, chunk: &'a [u8], env: &Env<'a>) -> Result<usize, LoadError> {
                let _ = (chunk, env);
                match k {
                    $( #[cfg(feature = $feat)] kind::$id => <$ty>::need(chunk, env), )*
                    k => Err(LoadError::UnsupportedKind(k)),
                }
            }
            fn load(k: u8, chunk: &'a [u8], env: &Env<'a>, pool: &mut Pool<'a>, rng: &mut dyn Rng)
                -> Result<Self, LoadError> {
                let _ = (chunk, env, &pool, &rng);
                crate::helpers::init();
                match k {
                    $( #[cfg(feature = $feat)] kind::$id => {
                        let mut d = <$ty>::new(chunk, env, pool, rng)?;
                        set_params(&mut d, param_bytes(k, chunk));
                        Ok(WsDevice::$var(d))
                    } )*
                    k => Err(LoadError::UnsupportedKind(k)),
                }
            }
        }
    };
}

ws_devices! {
    "ws-falcon" FALCON Falcon crate::falcon::Falcon<'a>,
    "ws-slaughter" SLAUGHTER Slaughter crate::slaughter::Slaughter<'a>,
    "ws-thunder" THUNDER Thunder crate::thunder::Thunder<'a>,
    "ws-scissor" SCISSOR Scissor crate::scissor::Scissor,
    "ws-leveller" LEVELLER Leveller crate::leveller::Leveller,
    "ws-crusher" CRUSHER Crusher crate::crusher::Crusher,
    "ws-echo" ECHO Echo crate::echo::Echo<'a>,
    "ws-smasher" SMASHER Smasher crate::smasher::Smasher<'a>,
    "ws-chamber" CHAMBER Chamber crate::chamber::Chamber<'a>,
    "ws-twister" TWISTER Twister crate::twister::Twister<'a>,
    "ws-cathedral" CATHEDRAL Cathedral crate::cathedral::Cathedral<'a>,
    "ws-adultery" ADULTERY Adultery crate::adultery::Adultery<'a>,
    "ws-specimen" SPECIMEN Specimen crate::specimen::Specimen<'a>,
}

#[allow(unused_variables)] // all arms compiled out without features
impl Device for WsDevice<'_> {
    fn param_count(&self) -> usize {
        each!(self, d => d.param_count())
    }
    fn set_param(&mut self, index: usize, value: f32) {
        each!(self, d => d.set_param(index, value))
    }
    fn note_on(&mut self, note: u8, velocity: u8, offset: u32) {
        each!(self, d => d.note_on(note, velocity, offset))
    }
    fn note_off(&mut self, note: u8, offset: u32) {
        each!(self, d => d.note_off(note, offset))
    }
    fn all_notes_off(&mut self) {
        each!(self, d => d.all_notes_off())
    }
    fn event_lookahead(&self) -> u32 {
        0
    }
    fn reset(&mut self) {
        each!(self, d => d.reset())
    }
    fn run(&mut self, io: &mut Io, n: usize, ctx: &mut Ctx) {
        each!(self, d => d.run(io, n, ctx))
    }
}
