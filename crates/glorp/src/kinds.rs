//! The Glorp devices as a `synth_core` device registry: [`GlorpDevice`] implements `Device` + `Kinds`, so a song of
//! kinds 3, 7, 14, 20, 21, 22, 25, 27, 28, 30, 32, 34, 35 and 38 plays through `synth_core::mix::Player` (sub-block [`BLOCK`] = 128, 44.1 kHz).
//!
//! `Device::set_param` / `run` / `note_*` cannot report a refusal, so the safety is layered:
//!
//! 1. **Song preflight** (`Kinds::preflight`, called by `Player::new` and `load_devices`; see [`crate::preflight`]):
//!    chunks, settings, rate and every automation lane that could leave the supported settings of an owned kind are
//!    refused before a device exists (kinds this crate does not own are skipped here so an `Either` can hand them to
//!    another registry; used alone they are refused by `need` with `UnsupportedKind`, and `GlorpPlayer` refuses them
//!    in its own strict preflight with the device index). `Kinds::block` makes `Player::new` refuse a sub-block other than 128 for a song holding an owned device.
//! 2. **`Kinds::need` / `load`** re-judge their own chunk (a direct caller cannot skip it).
//! 3. **Fault latch** for what is left (a `Device` driven by hand, an `Io` of another length, another rate): the first
//!    unsupported call is recorded in [`GlorpDevice::fault`], the call is **not** forwarded to the device and every
//!    later `run` writes silence into the main pair. Nothing renders audio from a setting the device does not model.
//!    The latch survives `reset`.
//!
//! The wrapped devices keep their own contract (see their module docs); this module only routes: instruments
//! overwrite main L/R, effects process it in place, ID 7 reads the side-chain pair when its P0 selects it, notes reach
//! instruments only (effects ignore them: observed inert, ID 38 needs its neutral envelope amount, enforced by the
//! preflight). `reset` rebuilds the device from its current parameters, which equals a freshly loaded device (tests).

use crate::preflight::{
    check_params, chunk_params, param_count, reason_name, Params, Reason, Reject, BLOCK, MARGIN, RATE,
};
use crate::{id14::Id14, id20::Id20, id21::Id21, id22::Id22, id25::Id25, id27::Id27, id28::Id28, id3::Id3, id30::Id30, id32::Id32, id34::Id34, id35::Id35, id38::Id38, id7::Id7};
use synth_core::device::{Ctx, Device, Io, LaneLerp, MAIN_L, MAIN_R};
use synth_core::mix::{Env, Kinds, LoadError, Song};
use synth_core::pool::Pool;
use synth_core::rng::Rng;

/// Why a device stopped rendering (text of the first unsupported call).
pub type Fault = &'static str;

/// One wrapped device kind (implementation detail of [`Shell`]; not meant to be implemented elsewhere).
#[doc(hidden)]
pub trait Core: Sized {
    const KIND: u8;
    const INSTRUMENT: bool;
    fn make(p: &Params) -> Result<Self, Fault>;
    fn set(&mut self, i: usize, v: f32) -> Result<(), Fault>;
    fn run(&mut self, l: &mut [f32], r: &mut [f32], sl: &[f32], sr: &[f32]) -> Result<(), Fault>;
    fn note_on(&mut self, _note: u8, _velocity: u8, _offset: u32) -> Result<(), Fault> {
        Ok(())
    }
    fn note_off(&mut self, _note: u8, _offset: u32) -> Result<(), Fault> {
        Ok(())
    }
    /// Fresh dynamic state under the parameters `p` (the device's current ones): `reset` / `all_notes_off`.
    fn rebuild(&mut self, p: &Params) -> Result<(), Fault> {
        *self = Self::make(p)?;
        Ok(())
    }
}

const SET: Fault = "the device refused a parameter";

macro_rules! effect {
    ($ty:ty, $kind:expr, $n:expr) => {
        impl Core for $ty {
            const KIND: u8 = $kind;
            const INSTRUMENT: bool = false;
            fn make(p: &Params) -> Result<Self, Fault> {
                <$ty>::new(p[..$n].first_chunk::<$n>().unwrap_or_else(|| unreachable!())).map_err(|_| SET)
            }
            fn set(&mut self, i: usize, v: f32) -> Result<(), Fault> {
                self.set_param(i, v).map_err(|_| SET)
            }
            fn run(&mut self, l: &mut [f32], r: &mut [f32], _: &[f32], _: &[f32]) -> Result<(), Fault> {
                self.process(l, r);
                Ok(())
            }
        }
    };
}

macro_rules! instrument {
    ($ty:ty, $kind:expr, $n:expr, $err:ty) => {
        impl Core for $ty {
            const KIND: u8 = $kind;
            const INSTRUMENT: bool = true;
            fn make(p: &Params) -> Result<Self, Fault> {
                <$ty>::new(p[..$n].first_chunk::<$n>().unwrap_or_else(|| unreachable!())).map_err(|_| SET)
            }
            fn set(&mut self, i: usize, v: f32) -> Result<(), Fault> {
                self.set_param(i, v).map_err(|_| SET)
            }
            fn run(&mut self, l: &mut [f32], r: &mut [f32], _: &[f32], _: &[f32]) -> Result<(), Fault> {
                self.process(l, r);
                Ok(())
            }
            // a full queue loses the event exactly as the original does; any other refusal is a fault
            fn note_on(&mut self, note: u8, _velocity: u8, offset: u32) -> Result<(), Fault> {
                match <$ty>::note_on(self, note, offset) {
                    Ok(()) | Err(<$err>::QueueFull) => Ok(()),
                    Err(_) => Err("note event refused"),
                }
            }
            fn note_off(&mut self, note: u8, offset: u32) -> Result<(), Fault> {
                match <$ty>::note_off(self, note, offset) {
                    Ok(()) | Err(<$err>::QueueFull) => Ok(()),
                    Err(_) => Err("note event refused"),
                }
            }
        }
    };
}

effect!(Id3, 3, 11);
effect!(Id34, 34, 4);
effect!(Id35, 35, 6);
effect!(Id38, 38, 15);
instrument!(Id14, 14, 26, crate::id14::Error);
instrument!(Id21, 21, 13, crate::id21::Error);
instrument!(Id22, 22, 15, crate::id22::Error);
instrument!(Id25, 25, 15, crate::id25::Error);
instrument!(Id27, 27, 13, crate::id27::Error);

/// ID 20 is the one instrument that reads the velocity (a velocity above its ceiling is refused by the song preflight;
/// a hand-written one latches here).
impl Core for Id20 {
    const KIND: u8 = 20;
    const INSTRUMENT: bool = true;
    fn make(p: &Params) -> Result<Self, Fault> {
        Id20::new(p[..16].first_chunk::<16>().unwrap_or_else(|| unreachable!())).map_err(|_| SET)
    }
    fn set(&mut self, i: usize, v: f32) -> Result<(), Fault> {
        self.set_param(i, v).map_err(|_| SET)
    }
    fn run(&mut self, l: &mut [f32], r: &mut [f32], _: &[f32], _: &[f32]) -> Result<(), Fault> {
        self.process(l, r);
        Ok(())
    }
    fn note_on(&mut self, note: u8, velocity: u8, offset: u32) -> Result<(), Fault> {
        match Id20::note_on(self, note, velocity, offset) {
            Ok(()) | Err(crate::id20::Error::QueueFull) => Ok(()),
            Err(_) => Err("note event refused"),
        }
    }
    fn note_off(&mut self, note: u8, offset: u32) -> Result<(), Fault> {
        match Id20::note_off(self, note, offset) {
            Ok(()) | Err(crate::id20::Error::QueueFull) => Ok(()),
            Err(_) => Err("note event refused"),
        }
    }
}

/// ID 28 reads the velocity (`velocity/127` scales its oscillators; every value of the song format, 0..=255, is finite).
impl Core for Id28 {
    const KIND: u8 = 28;
    const INSTRUMENT: bool = true;
    fn make(p: &Params) -> Result<Self, Fault> {
        Id28::new(p[..26].first_chunk::<26>().unwrap_or_else(|| unreachable!())).map_err(|_| SET)
    }
    fn set(&mut self, i: usize, v: f32) -> Result<(), Fault> {
        self.set_param(i, v).map_err(|_| SET)
    }
    fn run(&mut self, l: &mut [f32], r: &mut [f32], _: &[f32], _: &[f32]) -> Result<(), Fault> {
        self.process(l, r);
        Ok(())
    }
    fn note_on(&mut self, note: u8, velocity: u8, offset: u32) -> Result<(), Fault> {
        match Id28::note_on(self, note, velocity, offset) {
            Ok(()) | Err(crate::id28::Error::QueueFull) => Ok(()),
            Err(_) => Err("note event refused"),
        }
    }
    fn note_off(&mut self, note: u8, offset: u32) -> Result<(), Fault> {
        match Id28::note_off(self, note, offset) {
            Ok(()) | Err(crate::id28::Error::QueueFull) => Ok(()),
            Err(_) => Err("note event refused"),
        }
    }
}

impl Core for Id7 {
    const KIND: u8 = 7;
    const INSTRUMENT: bool = false;
    fn make(p: &Params) -> Result<Self, Fault> {
        Id7::new(p[..7].first_chunk::<7>().unwrap_or_else(|| unreachable!())).map_err(|_| SET)
    }
    fn set(&mut self, i: usize, v: f32) -> Result<(), Fault> {
        self.set_param(i, v).map_err(|_| SET)
    }
    fn run(&mut self, l: &mut [f32], r: &mut [f32], sl: &[f32], sr: &[f32]) -> Result<(), Fault> {
        let done = if self.sidechain() { self.process_sidechain(l, r, sl, sr) } else { self.process(l, r) };
        done.map_err(|_| "side-chain form refused")
    }
}

impl Core for Id32 {
    const KIND: u8 = 32;
    const INSTRUMENT: bool = false;
    fn make(p: &Params) -> Result<Self, Fault> {
        Id32::new(p[..11].first_chunk::<11>().unwrap_or_else(|| unreachable!())).map_err(|_| SET)
    }
    fn set(&mut self, i: usize, v: f32) -> Result<(), Fault> {
        self.set_param(i, v).map_err(|_| SET)
    }
    fn run(&mut self, l: &mut [f32], r: &mut [f32], _: &[f32], _: &[f32]) -> Result<(), Fault> {
        self.process(l, r).map_err(|_| "mode A delay exceeds the delay line")
    }
}

/// ID 30 borrows its ≈ 4 MB of delay lines from the song pool, so it cannot be built from the parameters alone
/// ([`Shell::in_pool`] builds it); `rebuild` clears the borrowed lines in place.
impl<'a> Core for Id30<'a> {
    const KIND: u8 = 30;
    const INSTRUMENT: bool = false;
    fn make(_: &Params) -> Result<Self, Fault> {
        Err("device needs the song pool")
    }
    fn set(&mut self, i: usize, v: f32) -> Result<(), Fault> {
        self.set_param(i, v).map_err(|_| SET)
    }
    fn run(&mut self, l: &mut [f32], r: &mut [f32], _: &[f32], _: &[f32]) -> Result<(), Fault> {
        self.process(l, r);
        Ok(())
    }
    fn rebuild(&mut self, _: &Params) -> Result<(), Fault> {
        self.reset();
        Ok(())
    }
}

/// A device with its current parameters (for `reset` and for judging a write before the device sees it) and its fault.
pub struct Shell<D> {
    dev: D,
    p: Params,
    fault: Option<Fault>,
}

impl<D: Core> Shell<D> {
    fn new(p: &Params) -> Result<Self, Fault> {
        Ok(Shell { dev: D::make(p)?, p: *p, fault: None })
    }

    fn latch(&mut self, f: Fault) {
        self.fault.get_or_insert(f);
    }

    fn set_param(&mut self, i: usize, v: f32) {
        if i >= param_count(D::KIND).unwrap_or(0) {
            return self.latch("parameter index outside the device");
        }
        // The player's interpolation can land an ulp outside [0, 1]; anything further out is not a lane value.
        if !v.is_finite() || !(-MARGIN..=1.0 + MARGIN).contains(&v) {
            return self.latch(reason_name(Reason::Range));
        }
        let v = v.clamp(0.0, 1.0);
        let mut next = self.p;
        next[i] = v;
        if let Err((_, why)) = check_params(D::KIND, &next) {
            return self.latch(reason_name(why));
        }
        match self.dev.set(i, v) {
            Ok(()) => self.p = next,
            Err(f) => self.latch(f),
        }
    }

    /// `io` buffers hold ≥ `n` frames (the `Io` contract; the mixer passes exactly `n`), so the `[..n]` slices are
    /// in range for every caller that keeps it.
    fn run(&mut self, io: &mut Io, n: usize, ctx: &Ctx) {
        if n != BLOCK {
            self.latch("render call is not 128 frames");
        } else if ctx.rate != RATE as f64 {
            self.latch("render rate is not 44100");
        }
        if self.fault.is_none() {
            let [l, r, sl, sr] = &mut io.ch;
            if let Err(f) = self.dev.run(&mut l[..n], &mut r[..n], &sl[..n], &sr[..n]) {
                self.latch(f);
            }
        }
        if self.fault.is_some() {
            io.ch[MAIN_L][..n].fill(0.0);
            io.ch[MAIN_R][..n].fill(0.0);
        }
    }

    fn note_on(&mut self, note: u8, velocity: u8, offset: u32) {
        if D::INSTRUMENT && self.fault.is_none() {
            if let Err(f) = self.dev.note_on(note, velocity, offset) {
                self.latch(f);
            }
        }
    }

    fn note_off(&mut self, note: u8, offset: u32) {
        if D::INSTRUMENT && self.fault.is_none() {
            if let Err(f) = self.dev.note_off(note, offset) {
                self.latch(f);
            }
        }
    }

    /// Fresh dynamic state, current parameters. The fault latch is deliberately **not** cleared: a refused write,
    /// note or call was never applied to `self.p` / the device, so rebuilding from `self.p` would silently play on
    /// as if it had not happened. The module doc ("survives `reset`") and the tests pin this.
    fn reset(&mut self) {
        if let Err(f) = self.dev.rebuild(&self.p) {
            self.latch(f);
        }
    }
}

impl<'a> Shell<Id30<'a>> {
    /// Builds the device, carving its lines from `pool`.
    fn in_pool(p: &Params, pool: &mut Pool<'a>) -> Result<Self, LoadError> {
        let dev = Id30::new(p[..24].first_chunk::<24>().unwrap_or_else(|| unreachable!()), pool).map_err(|e| match e {
            crate::id30::Error::Memory => LoadError::OutOfMemory,
            _ => LoadError::BadChunk,
        })?;
        Ok(Shell { dev, p: *p, fault: None })
    }
}

/// The device kinds of this crate, carved from the song's pool.
pub enum GlorpDevice<'a> {
    Id3(&'a mut Shell<Id3>),
    Id7(&'a mut Shell<Id7>),
    Id14(&'a mut Shell<Id14>),
    Id20(&'a mut Shell<Id20>),
    Id21(&'a mut Shell<Id21>),
    Id22(&'a mut Shell<Id22>),
    Id25(&'a mut Shell<Id25>),
    Id27(&'a mut Shell<Id27>),
    Id28(&'a mut Shell<Id28>),
    Id30(&'a mut Shell<Id30<'a>>),
    Id32(&'a mut Shell<Id32>),
    Id34(&'a mut Shell<Id34>),
    Id35(&'a mut Shell<Id35>),
    Id38(&'a mut Shell<Id38>),
}

macro_rules! each {
    ($s:expr, $d:ident => $e:expr) => {
        match $s {
            GlorpDevice::Id3($d) => $e,
            GlorpDevice::Id7($d) => $e,
            GlorpDevice::Id14($d) => $e,
            GlorpDevice::Id20($d) => $e,
            GlorpDevice::Id21($d) => $e,
            GlorpDevice::Id22($d) => $e,
            GlorpDevice::Id25($d) => $e,
            GlorpDevice::Id27($d) => $e,
            GlorpDevice::Id28($d) => $e,
            GlorpDevice::Id30($d) => $e,
            GlorpDevice::Id32($d) => $e,
            GlorpDevice::Id34($d) => $e,
            GlorpDevice::Id35($d) => $e,
            GlorpDevice::Id38($d) => $e,
        }
    };
}

impl GlorpDevice<'_> {
    /// Song device kind (3, 7, 14, 20, 21, 22, 25, 27, 28, 30, 32, 34, 35 or 38).
    pub fn kind(&self) -> u8 {
        match self {
            GlorpDevice::Id3(_) => 3,
            GlorpDevice::Id7(_) => 7,
            GlorpDevice::Id14(_) => 14,
            GlorpDevice::Id20(_) => 20,
            GlorpDevice::Id21(_) => 21,
            GlorpDevice::Id22(_) => 22,
            GlorpDevice::Id25(_) => 25,
            GlorpDevice::Id27(_) => 27,
            GlorpDevice::Id28(_) => 28,
            GlorpDevice::Id30(_) => 30,
            GlorpDevice::Id32(_) => 32,
            GlorpDevice::Id34(_) => 34,
            GlorpDevice::Id35(_) => 35,
            GlorpDevice::Id38(_) => 38,
        }
    }

    /// The first unsupported call this device saw, if any; it renders silence from then on.
    pub fn fault(&self) -> Option<Fault> {
        each!(self, d => d.fault)
    }
}

impl Device for GlorpDevice<'_> {
    fn param_count(&self) -> usize {
        param_count(self.kind()).unwrap_or(0)
    }
    /// The original engine writes every lane as `a + (b−a)·t` (measured: 0 mismatches of 6,028,484 recorded writes of
    /// every automated lane of the real song; the core's `a·(1−t) + b·t` is one ulp off in ~6.7 %, which ID 21 turns into
    /// other partials because it hashes the bits of P7). All kinds: the form belongs to the engine, not to a device.
    fn lane_lerp(&self) -> LaneLerp {
        LaneLerp::Delta
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
    /// Instruments restart from a fresh state (voices and queue dropped); effects hold no notes.
    fn all_notes_off(&mut self) {
        match self {
            GlorpDevice::Id14(d) => d.reset(),
            GlorpDevice::Id20(d) => d.reset(),
            GlorpDevice::Id21(d) => d.reset(),
            GlorpDevice::Id22(d) => d.reset(),
            GlorpDevice::Id25(d) => d.reset(),
            GlorpDevice::Id27(d) => d.reset(),
            GlorpDevice::Id28(d) => d.reset(),
            _ => {}
        }
    }
    fn reset(&mut self) {
        each!(self, d => d.reset())
    }
    fn run(&mut self, io: &mut Io, n: usize, ctx: &mut Ctx) {
        each!(self, d => d.run(io, n, ctx))
    }
}

fn read(kind: u8, chunk: &[u8], env: &Env<'_>) -> Result<Params, LoadError> {
    if param_count(kind).is_none() {
        return Err(LoadError::UnsupportedKind(kind));
    }
    if env.rate != RATE as f64 {
        return Err(LoadError::UnsupportedModule(kind));
    }
    chunk_params(kind, chunk).map_err(|(p, r)| Reject::at(0, kind, p, r).load_error())
}

impl<'a> Kinds<'a> for GlorpDevice<'a> {
    fn block(song: &Song<'a>, _: &Env<'a>) -> Result<Option<usize>, LoadError> {
        // pinned only for a song that holds an owned device: a foreign-only song (an `Either` partner's) is not ours to pin
        Ok(song.devices().any(|d| param_count(d.kind).is_some()).then_some(BLOCK))
    }

    fn preflight(song: &Song<'a>, env: &Env<'a>) -> Result<(), LoadError> {
        // owned kinds only (the `Kinds::preflight` contract, so an `Either` can pair this registry with a disjoint one);
        // an unowned kind is refused by `need` below. `GlorpPlayer` uses the strict `preflight` instead.
        crate::preflight::preflight_owned(song, env).map_err(|r| r.load_error())
    }

    fn need(kind: u8, chunk: &'a [u8], env: &Env<'a>) -> Result<usize, LoadError> {
        read(kind, chunk, env)?;
        Ok(match kind {
            3 => Pool::need::<Shell<Id3>>(1),
            7 => Pool::need::<Shell<Id7>>(1),
            14 => Pool::need::<Shell<Id14>>(1),
            20 => Pool::need::<Shell<Id20>>(1),
            21 => Pool::need::<Shell<Id21>>(1),
            22 => Pool::need::<Shell<Id22>>(1),
            25 => Pool::need::<Shell<Id25>>(1),
            27 => Pool::need::<Shell<Id27>>(1),
            28 => Pool::need::<Shell<Id28>>(1),
            30 => Pool::need::<Shell<Id30>>(1) + Pool::need::<f32>(crate::id30::SLAB_F32),
            32 => Pool::need::<Shell<Id32>>(1),
            34 => Pool::need::<Shell<Id34>>(1),
            35 => Pool::need::<Shell<Id35>>(1),
            _ => Pool::need::<Shell<Id38>>(1),
        })
    }

    fn load(kind: u8, chunk: &'a [u8], env: &Env<'a>, pool: &mut Pool<'a>, _: &mut dyn Rng) -> Result<Self, LoadError> {
        let p = read(kind, chunk, env)?;
        macro_rules! build {
            ($v:ident, $t:ty) => {
                GlorpDevice::$v(pool.value(Shell::<$t>::new(&p).map_err(|_| LoadError::BadChunk)?)?)
            };
        }
        Ok(match kind {
            3 => build!(Id3, Id3),
            7 => build!(Id7, Id7),
            14 => build!(Id14, Id14),
            20 => build!(Id20, Id20),
            21 => build!(Id21, Id21),
            22 => build!(Id22, Id22),
            25 => build!(Id25, Id25),
            27 => build!(Id27, Id27),
            28 => build!(Id28, Id28),
            30 => {
                let shell = Shell::<Id30<'a>>::in_pool(&p, pool)?;
                GlorpDevice::Id30(pool.value(shell)?)
            }
            32 => build!(Id32, Id32),
            34 => build!(Id34, Id34),
            35 => build!(Id35, Id35),
            _ => build!(Id38, Id38),
        })
    }
}
