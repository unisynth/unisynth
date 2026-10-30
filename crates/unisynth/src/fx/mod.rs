//! The unified effects (device kinds 65–73). One kind per effect family; its first parameter selects the *mode*, a
//! original algorithm kept whole: the parameters after it are that effect's, in its order and with its
//! normalized ranges and curves, so the original effect's parameters and their automation convert by copying.
//!
//! | kind | modes (mode parameter `choice(p, n)`) |
//! |---|---|
//! | 65 distortion | 0 shaper (WaveSabre Scissor), 1 curve (Screwdriver, qm210 WaveSabre fork), 2 power (Tunefish) |
//! | 66 crusher | 0 crusher (WaveSabre) |
//! | 67 EQ | 0 parametric (WaveSabre Leveller), 1 3band (Tunefish) |
//! | 68 delay | 0 tempo (WaveSabre Echo), 1 ms (Tunefish) |
//! | 69 chorus | 0 chorus (Tunefish) |
//! | 70 flanger | 0 phaser (WaveSabre Twister), 1 ring (Tunefish) |
//! | 71 reverb | 0 freeverb (WaveSabre Cathedral), 1 compact (Tunefish), 2 rooms (WaveSabre Chamber) |
//! | 72 compressor | 0 compressor (WaveSabre Smasher) |
//! | 73 formant | 0 formant (Tunefish) |
//!
//! V2 modes (change add-v2-import, `fx/v2.rs`): 67 EQ 2 cut, 68 delay 2 v2, 71 reverb 3 v2, 72 compressor 1 v2.
//! Mini-cauldron modes (change add-cauldron-xm-import): 65 distortion 3 wrap (`fx/wrap.rs`), 70 flanger 2 comb
//! (`fx/comb.rs`).
//!
//! Every (kind, mode) is a Cargo feature (`us-fx-*`); a chunk that selects a mode the build lacks fails at load with
//! `UnsupportedModule` naming it. The mode is chosen by the chunk only (`set_param(0, …)` is ignored): it sizes the
//! effect's memory. Effects transform main L/R in place, never write the sidechain pair, take the rate and tempo from
//! the render context, and give the same output for any split of a render into calls.
//!
//! A chunk is the original device chunk with the mode in front: little-endian f32 parameters (`[mode, p0, p1, …]`)
//! followed by a 4-byte trailer that is ignored; missing parameters keep their defaults.

use crate::kind;
use crate::law as plaw;
use crate::module as m;
use core::marker::PhantomData;
use synth_core::device::{Ctx, Device, Io};
use synth_core::mix::{Env, LoadError};
use synth_core::pool::Pool;

mod law;
#[cfg(feature = "us-fx-reverb-freeverb")]
mod freeverb;
#[cfg(feature = "us-fx-reverb-rooms")]
mod rooms;
#[cfg(feature = "us-fx-crusher")]
mod crusher;
#[cfg(feature = "us-fx-delay-tempo")]
mod tempo;
#[cfg(feature = "us-fx-eq-parametric")]
mod parametric;
#[cfg(feature = "us-fx-distortion-shaper")]
mod shaper;
#[cfg(any(feature = "us-fx-reverb-freeverb", feature = "us-fx-reverb-compact"))]
mod reverb;
#[cfg(feature = "us-fx-compressor")]
mod compressor;
#[cfg(any(
    feature = "us-fx-delay-tempo",
    feature = "us-fx-flanger-phaser",
    feature = "us-fx-reverb-freeverb",
    feature = "us-fx-reverb-rooms"
))]
mod svf;
#[cfg(feature = "us-fx-flanger-phaser")]
mod phaser;
#[cfg(feature = "us-fx-chorus")]
mod chorus;
#[cfg(feature = "us-fx-delay-ms")]
mod delay_ms;
#[cfg(feature = "us-fx-distortion-power")]
mod power;
#[cfg(feature = "us-fx-distortion-curve")]
mod curve;
#[cfg(feature = "us-fx-eq-3band")]
mod three_band;
#[cfg(feature = "us-fx-flanger-ring")]
mod ring;
#[cfg(feature = "us-fx-formant")]
mod formant;
#[cfg(feature = "us-fx-flanger-comb")]
mod comb;
#[cfg(feature = "us-fx-distortion-wrap")]
mod wrap;
#[cfg(feature = "us-fx-reverb-compact")]
mod compact;
#[cfg(any(feature = "us-fx-eq-cut", feature = "us-fx-delay-modulated", feature = "us-fx-reverb-comb", feature = "us-fx-compressor-lookahead"))]
mod v2;

/// The effect's own parameters of a chunk (the values after the mode), for sizing memory before the effect exists.
pub struct Params<'c>(&'c [u8]);

impl<'c> Params<'c> {
    /// Parameter `i` of the effect (chunk value `i + 1`), if the chunk carries it.
    pub fn get(&self, i: usize) -> Option<f32> {
        value(self.0, i + 1)
    }
}

fn value(chunk: &[u8], i: usize) -> Option<f32> {
    let n = chunk.len().saturating_sub(4) / 4;
    (i < n).then(|| {
        let b = &chunk[i * 4..i * 4 + 4];
        f32::from_le_bytes([b[0], b[1], b[2], b[3]])
    })
}

/// One mode's implementation: an original effect body.
#[allow(dead_code)] // unused in a build with no effect modes
trait Effect<'a>: Sized {
    /// Parameters after the mode.
    const PARAMS: usize;
    /// Pool bytes the mode needs (an upper bound), from the song's rate and tempo and the chunk's parameters.
    fn need(env: &Env, p: &Params) -> usize;
    fn new(env: &Env, pool: &mut Pool<'a>) -> Result<Self, LoadError>;
    /// Parameter `i` of the effect (after the mode).
    fn set(&mut self, i: usize, v: f32);
    fn reset(&mut self);
    fn run(&mut self, io: &mut Io, n: usize, ctx: &mut Ctx);
}

/// `(kind, mode, module id)` for every mode of every kind, in mode order.
// Distortion: curve sits between shaper and power so the stored mode values keep their meaning (0 = shaper,
// 1.0 = power, as before curve existed; curve = 0.5). The V2 modes (change add-v2-import) come after the modes of
// chunk v1 ([`OLD_MODES`]) and are stored between them (`params::kinds`), so old values keep their meaning too.
const MODES: [(u8, usize, u8); 22] = [
    (kind::DISTORTION, 0, m::FX_DISTORTION_SHAPER),
    (kind::DISTORTION, 1, m::FX_DISTORTION_CURVE),
    (kind::DISTORTION, 2, m::FX_DISTORTION_POWER),
    (kind::CRUSHER, 0, m::FX_CRUSHER),
    (kind::EQ, 0, m::FX_EQ_PARAMETRIC),
    (kind::EQ, 1, m::FX_EQ_3BAND),
    (kind::DELAY, 0, m::FX_DELAY_TEMPO),
    (kind::DELAY, 1, m::FX_DELAY_MS),
    (kind::CHORUS, 0, m::FX_CHORUS),
    (kind::FLANGER, 0, m::FX_FLANGER_PHASER),
    (kind::FLANGER, 1, m::FX_FLANGER_RING),
    (kind::REVERB, 0, m::FX_REVERB_FREEVERB),
    (kind::REVERB, 1, m::FX_REVERB_COMPACT),
    (kind::REVERB, 2, m::FX_REVERB_ROOMS),
    (kind::COMPRESSOR, 0, m::FX_COMPRESSOR),
    (kind::FORMANT, 0, m::FX_FORMANT),
    (kind::EQ, 2, m::FX_EQ_CUT),
    (kind::DELAY, 2, m::FX_DELAY_MODULATED),
    (kind::REVERB, 3, m::FX_REVERB_COMB),
    (kind::COMPRESSOR, 1, m::FX_COMPRESSOR_LOOKAHEAD),
    // change add-cauldron-xm-import: stored at the free positions (distortion 0.25, flanger 0.5)
    (kind::DISTORTION, 3, m::FX_DISTORTION_WRAP),
    (kind::FLANGER, 2, m::FX_FLANGER_COMB),
];

/// Modes per kind before change add-v2-import (`inv_choice(mode, mode_count(k))` still encodes them).
const OLD_MODES: [(u8, usize); 9] = [
    (kind::DISTORTION, 3),
    (kind::CRUSHER, 1),
    (kind::EQ, 2),
    (kind::DELAY, 2),
    (kind::CHORUS, 1),
    (kind::FLANGER, 2),
    (kind::REVERB, 3),
    (kind::COMPRESSOR, 1),
    (kind::FORMANT, 1),
];

/// Number of modes of effect kind `k` that chunk v1 had (0 for a kind that is not an effect): `inv_choice(mode,
/// mode_count(k))` encodes those. [`mode_total`] counts the later ones too; [`mode_value`] encodes any mode.
pub fn mode_count(k: u8) -> usize {
    OLD_MODES.iter().find(|e| e.0 == k).map_or(0, |e| e.1)
}

/// Number of modes of effect kind `k`, the V2 ones included.
pub fn mode_total(k: u8) -> usize {
    MODES.iter().filter(|e| e.0 == k).count()
}

/// The mode a stored mode value selects: an expanded choice (`params::kinds`) when the kind gained modes.
#[inline]
pub fn mode_of_value(k: u8, p: f32) -> usize {
    // a trusted build without the V2 modes reads only old ones (see `params::kinds::decode_in`)
    let v2 = cfg!(feature = "us-fx-eq-cut")
        || cfg!(feature = "us-fx-delay-modulated")
        || cfg!(feature = "us-fx-reverb-comb")
        || cfg!(feature = "us-fx-compressor-lookahead")
        || cfg!(feature = "us-fx-distortion-wrap")
        || cfg!(feature = "us-fx-flanger-comb");
    if cfg!(feature = "us-trusted") && !v2 {
        return plaw::choice(p, mode_count(k));
    }
    let (old, later) = (mode_count(k), mode_total(k) - mode_count(k));
    if later == 0 || old == 0 {
        plaw::choice(p, old.max(1))
    } else if old == 1 {
        plaw::choice(p, 1 + later)
    } else {
        crate::params::kinds::decode(p, old, later, 0)
    }
}

/// The stored value of mode `mode` of kind `k`.
pub fn mode_value(k: u8, mode: usize) -> f32 {
    let (old, later) = (mode_count(k), mode_total(k) - mode_count(k));
    if later == 0 {
        if old > 1 { mode as f32 / (old - 1) as f32 } else { 0.0 }
    } else if old == 1 {
        mode as f32 / later as f32
    } else {
        crate::params::kinds::encode(mode, old)
    }
}

/// Stored positions of kind `k`'s mode value (`choice(p, positions)`): the modes and, between the old ones, room
/// for the later ones.
pub fn mode_positions(k: u8) -> usize {
    let (old, later) = (mode_count(k), mode_total(k) - mode_count(k));
    if later == 0 {
        old
    } else if old == 1 {
        1 + later
    } else {
        2 * old - 1
    }
}

/// The mode at stored position `pos`, or `None` for a free position.
pub fn mode_at(k: u8, pos: usize) -> Option<usize> {
    let m = mode_of_value(k, if mode_positions(k) > 1 { pos as f32 / (mode_positions(k) - 1) as f32 } else { 0.0 });
    (mode_value(k, m) == if mode_positions(k) > 1 { pos as f32 / (mode_positions(k) - 1) as f32 } else { 0.0 }).then_some(m)
}

/// The mode a chunk selects.
pub fn mode_of(k: u8, chunk: &[u8]) -> usize {
    mode_of_value(k, value(chunk, 0).unwrap_or(0.0))
}

/// The module id of mode `mode` of effect kind `k`.
pub fn module_of(k: u8, mode: usize) -> Option<u8> {
    MODES.iter().find(|e| e.0 == k && e.1 == mode).map(|e| e.2)
}

/// True for the effect kinds 65–73.
pub fn is_effect(k: u8) -> bool {
    mode_total(k) > 0
}

fn missing(k: u8, mode: usize) -> LoadError {
    match MODES.iter().find(|e| e.0 == k && e.1 == mode) {
        Some(e) => LoadError::UnsupportedModule(e.2),
        None => LoadError::UnsupportedKind(k),
    }
}

macro_rules! modes {
    ($( $feat:literal $kind:ident $idx:literal $var:ident $ty:ty ),* $(,)?) => {
        enum Mode<'a> {
            $( #[cfg(feature = $feat)] $var($ty), )*
            #[allow(dead_code)]
            Never(PhantomData<&'a ()>, core::convert::Infallible),
        }

        impl<'a> Mode<'a> {
            #[allow(unused_variables)]
            fn need(k: u8, mode: usize, env: &Env, p: &Params) -> Result<usize, LoadError> {
                match (k, mode) {
                    $( #[cfg(feature = $feat)] (kind::$kind, $idx) => Ok(<$ty as Effect>::need(env, p)), )*
                    _ => Err(missing(k, mode)),
                }
            }
            #[allow(unused_variables)]
            fn create(k: u8, mode: usize, env: &Env, pool: &mut Pool<'a>) -> Result<(Self, usize), LoadError> {
                match (k, mode) {
                    $( #[cfg(feature = $feat)] (kind::$kind, $idx) => Ok((Mode::$var(<$ty as Effect>::new(env, pool)?), <$ty as Effect>::PARAMS)), )*
                    _ => Err(missing(k, mode)),
                }
            }
            #[allow(unused_variables)]
            fn set(&mut self, i: usize, v: f32) {
                match self {
                    $( #[cfg(feature = $feat)] Mode::$var(e) => e.set(i, v), )*
                    Mode::Never(_, n) => match *n {},
                }
            }
            fn reset(&mut self) {
                match self {
                    $( #[cfg(feature = $feat)] Mode::$var(e) => e.reset(), )*
                    Mode::Never(_, n) => match *n {},
                }
            }
            #[allow(unused_variables)]
            fn run(&mut self, io: &mut Io, n: usize, ctx: &mut Ctx) {
                match self {
                    $( #[cfg(feature = $feat)] Mode::$var(e) => e.run(io, n, ctx), )*
                    Mode::Never(_, n) => match *n {},
                }
            }
        }
    };
}

modes! {
    "us-fx-distortion-shaper" DISTORTION 0 Shaper shaper::Shaper,
    "us-fx-distortion-curve" DISTORTION 1 Curve curve::Curve,
    "us-fx-distortion-power" DISTORTION 2 Power power::Power,
    "us-fx-crusher" CRUSHER 0 Crusher crusher::Crusher,
    "us-fx-eq-parametric" EQ 0 Parametric parametric::Parametric,
    "us-fx-eq-3band" EQ 1 ThreeBand three_band::ThreeBand,
    "us-fx-delay-tempo" DELAY 0 Tempo tempo::Tempo<'a>,
    "us-fx-delay-ms" DELAY 1 DelayMs delay_ms::DelayMs<'a>,
    "us-fx-chorus" CHORUS 0 Chorus chorus::Chorus<'a>,
    "us-fx-flanger-phaser" FLANGER 0 Phaser phaser::Phaser<'a>,
    "us-fx-flanger-ring" FLANGER 1 RingFlanger ring::RingFlanger<'a>,
    "us-fx-reverb-freeverb" REVERB 0 FreeverbReverb freeverb::FreeverbReverb<'a>,
    "us-fx-reverb-compact" REVERB 1 Compact compact::Compact<'a>,
    "us-fx-reverb-rooms" REVERB 2 Rooms rooms::Rooms<'a>,
    "us-fx-compressor" COMPRESSOR 0 Compressor compressor::Compressor<'a>,
    "us-fx-formant" FORMANT 0 Formant formant::Formant,
    "us-fx-eq-cut" EQ 2 Cut v2::Cut,
    "us-fx-delay-modulated" DELAY 2 DelayV2 v2::DelayV2<'a>,
    "us-fx-reverb-comb" REVERB 3 ReverbV2 v2::ReverbV2<'a>,
    "us-fx-compressor-lookahead" COMPRESSOR 1 CompressorV2 v2::CompressorV2<'a>,
    "us-fx-distortion-wrap" DISTORTION 3 Wrap wrap::Wrap,
    "us-fx-flanger-comb" FLANGER 2 Comb comb::Comb<'a>,
}

/// An effect device (kinds 65–73).
pub struct UFx<'a> {
    mode: Mode<'a>,
    /// Parameters after the mode.
    params: usize,
}

impl<'a> UFx<'a> {
    /// Pool bytes for effect kind `k` loaded from `chunk`.
    pub fn need(k: u8, chunk: &[u8], env: &Env) -> Result<usize, LoadError> {
        Mode::need(k, mode_of(k, chunk), env, &Params(chunk))
    }

    pub fn new(k: u8, chunk: &[u8], env: &Env, pool: &mut Pool<'a>) -> Result<Self, LoadError> {
        let (mode, params) = Mode::create(k, mode_of(k, chunk), env, pool)?;
        let mut fx = UFx { mode, params };
        for i in 0..params {
            if let Some(v) = value(chunk, i + 1) {
                fx.mode.set(i, v);
            }
        }
        Ok(fx)
    }
}

#[cfg(all(test, feature = "us-fx-chorus"))]
impl UFx<'_> {
    /// The chorus's first line phase (tests).
    fn chorus_phase(&self) -> f32 {
        match &self.mode {
            Mode::Chorus(c) => c.phase(),
            _ => panic!("not the chorus"),
        }
    }
}

#[cfg(all(test, feature = "us-fx-flanger-ring"))]
impl UFx<'_> {
    /// The ring flanger's sweep phases (tests).
    fn tf_flanger_theta(&self) -> [f32; 2] {
        match &self.mode {
            Mode::RingFlanger(f) => f.theta(),
            _ => panic!("not the ring flanger"),
        }
    }
}

impl Device for UFx<'_> {
    fn param_count(&self) -> usize {
        1 + self.params
    }
    fn set_param(&mut self, index: usize, value: f32) {
        if index >= 1 && index <= self.params {
            self.mode.set(index - 1, value);
        }
    }
    fn reset(&mut self) {
        self.mode.reset();
    }
    fn run(&mut self, io: &mut Io, n: usize, ctx: &mut Ctx) {
        self.mode.run(io, n, ctx);
    }
}

/// Builds an effect chunk from its parameters (`[mode, p0, …]`, mode as a normalized value). Host side.
#[cfg(any(test, feature = "std"))]
pub fn encode(values: &[f32]) -> std::vec::Vec<u8> {
    let mut v: std::vec::Vec<u8> = values.iter().flat_map(|x| x.to_le_bytes()).collect();
    v.extend((v.len() as i32 + 4).to_le_bytes());
    v
}

#[cfg(test)]
mod golden;
#[cfg(test)]
mod tests;
