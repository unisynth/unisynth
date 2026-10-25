//! The instrument's 10 serial insert-effect slots (spec §4.0). Slot type `round(10p)`: 0 none, 1 distortion,
//! 2 delay, 3 chorus, 4 flanger, 5 reverb, 6 formant, 7 EQ, 8–10 none. All slots share the instrument's effect
//! parameters. A type change replaces the slot with a fresh zeroed instance; its memory comes from the
//! instrument's effect pool (a bump pool: a replaced instance's memory is not reused). A kind that is not compiled
//! in, or that no longer fits the pool, acts as none.

#[cfg(feature = "tf-fx-chorus")]
pub mod chorus;
#[cfg(feature = "tf-fx-delay")]
pub mod delay;
#[cfg(feature = "tf-fx-distortion")]
pub mod distortion;
#[cfg(feature = "tf-fx-eq")]
pub mod eq;
#[cfg(feature = "tf-fx-flanger")]
pub mod flanger;
#[cfg(feature = "tf-fx-formant")]
pub mod formant;
#[cfg(feature = "tf-fx-reverb")]
pub mod reverb;

use crate::params::{effect_kind, COUNT, EFFECT_FIRST};
use crate::rng::Xorshift;
use synth_core::pool::{OutOfMemory, Pool};

pub const SLOTS: usize = 10;

/// Effect kinds (slot type values).
pub mod kind {
    pub const NONE: u8 = 0;
    pub const DISTORTION: u8 = 1;
    pub const DELAY: u8 = 2;
    pub const CHORUS: u8 = 3;
    pub const FLANGER: u8 = 4;
    pub const REVERB: u8 = 5;
    pub const FORMANT: u8 = 6;
    pub const EQ: u8 = 7;
}

pub enum Fx<'a> {
    None,
    #[cfg(feature = "tf-fx-distortion")]
    Distortion(distortion::Distortion),
    #[cfg(feature = "tf-fx-delay")]
    Delay(delay::Delay<'a>),
    #[cfg(feature = "tf-fx-chorus")]
    Chorus(chorus::Chorus<'a>),
    #[cfg(feature = "tf-fx-flanger")]
    Flanger(flanger::Flanger<'a>),
    #[cfg(feature = "tf-fx-reverb")]
    Reverb(reverb::Reverb<'a>),
    #[cfg(feature = "tf-fx-formant")]
    Formant(formant::Formant),
    #[cfg(feature = "tf-fx-eq")]
    Eq(eq::Eq),
    #[doc(hidden)]
    _Lifetime(core::marker::PhantomData<&'a ()>),
}

impl<'a> Fx<'a> {
    /// Pool bytes one instance of `kind` needs (0 for none, pool-free or disabled kinds).
    pub fn need(kind: u8) -> usize {
        match kind {
            #[cfg(feature = "tf-fx-delay")]
            kind::DELAY => delay::Delay::need(),
            #[cfg(feature = "tf-fx-chorus")]
            kind::CHORUS => chorus::Chorus::need(),
            #[cfg(feature = "tf-fx-flanger")]
            kind::FLANGER => flanger::Flanger::need(),
            #[cfg(feature = "tf-fx-reverb")]
            kind::REVERB => reverb::Reverb::need(),
            _ => 0,
        }
    }

    /// A fresh instance of `kind`; `None` for none, reserved or disabled kinds.
    #[allow(unused_variables)]
    pub fn create(kind: u8, pool: &mut Pool<'a>, rng: &mut Xorshift) -> Result<Self, OutOfMemory> {
        Ok(match kind {
            #[cfg(feature = "tf-fx-distortion")]
            kind::DISTORTION => Fx::Distortion(distortion::Distortion),
            #[cfg(feature = "tf-fx-delay")]
            kind::DELAY => Fx::Delay(delay::Delay::new(pool)?),
            #[cfg(feature = "tf-fx-chorus")]
            kind::CHORUS => Fx::Chorus(chorus::Chorus::new(pool, rng)?),
            #[cfg(feature = "tf-fx-flanger")]
            kind::FLANGER => Fx::Flanger(flanger::Flanger::new(pool)?),
            #[cfg(feature = "tf-fx-reverb")]
            kind::REVERB => Fx::Reverb(reverb::Reverb::new(pool)?),
            #[cfg(feature = "tf-fx-formant")]
            kind::FORMANT => Fx::Formant(formant::Formant::default()),
            #[cfg(feature = "tf-fx-eq")]
            kind::EQ => Fx::Eq(eq::Eq::default()),
            _ => Fx::None,
        })
    }

    /// Back to a fresh instance's state, in place (no allocation); the chorus redraws its phases from `rng`.
    #[allow(unused_variables)]
    pub fn clear(&mut self, rng: &mut Xorshift) {
        match self {
            Fx::None | Fx::_Lifetime(_) => {}
            #[cfg(feature = "tf-fx-distortion")]
            Fx::Distortion(_) => {}
            #[cfg(feature = "tf-fx-delay")]
            Fx::Delay(f) => f.clear(),
            #[cfg(feature = "tf-fx-chorus")]
            Fx::Chorus(f) => f.clear(rng),
            #[cfg(feature = "tf-fx-flanger")]
            Fx::Flanger(f) => f.clear(),
            #[cfg(feature = "tf-fx-reverb")]
            Fx::Reverb(f) => f.clear(),
            #[cfg(feature = "tf-fx-formant")]
            Fx::Formant(f) => *f = Default::default(),
            #[cfg(feature = "tf-fx-eq")]
            Fx::Eq(f) => *f = Default::default(),
        }
    }

    /// Processes one 512-frame instrument block in place.
    #[allow(unused_variables)]
    pub fn run(&mut self, p: &[f32; COUNT], l: &mut [f32], r: &mut [f32]) {
        match self {
            Fx::None | Fx::_Lifetime(_) => {}
            #[cfg(feature = "tf-fx-distortion")]
            Fx::Distortion(f) => f.run(p, l, r),
            #[cfg(feature = "tf-fx-delay")]
            Fx::Delay(f) => f.run(p, l, r),
            #[cfg(feature = "tf-fx-chorus")]
            Fx::Chorus(f) => {
                f.control(p);
                f.process(l, r);
            }
            #[cfg(feature = "tf-fx-flanger")]
            Fx::Flanger(f) => f.run(p, l, r),
            #[cfg(feature = "tf-fx-reverb")]
            Fx::Reverb(f) => f.run(p, l, r),
            #[cfg(feature = "tf-fx-formant")]
            Fx::Formant(f) => f.run(p, l, r),
            #[cfg(feature = "tf-fx-eq")]
            Fx::Eq(f) => f.run(p, l, r),
        }
    }
}

/// The 10-slot chain with its own effect pool.
pub struct Chain<'a> {
    slots: [Fx<'a>; SLOTS],
    kinds: [u8; SLOTS],
    /// Slots to return to fresh state at their next run (after `reset`, where a fresh load would create them).
    stale: [bool; SLOTS],
    pool: Pool<'a>,
}

impl<'a> Chain<'a> {
    pub fn new(pool: Pool<'a>) -> Self {
        Chain { slots: core::array::from_fn(|_| Fx::None), kinds: [kind::NONE; SLOTS], stale: [false; SLOTS], pool }
    }

    /// Effect-pool bytes for the slot kinds of patch `p` (one instance per used slot).
    pub fn need(p: &[f32; COUNT]) -> usize {
        (0..SLOTS).map(|i| Fx::need(effect_kind(p[EFFECT_FIRST + i]))).sum()
    }

    /// Runs the slots in order; a slot whose type changed is first replaced by a fresh instance.
    pub fn run(&mut self, p: &[f32; COUNT], rng: &mut Xorshift, l: &mut [f32], r: &mut [f32]) {
        for i in 0..SLOTS {
            let k = effect_kind(p[EFFECT_FIRST + i]);
            if k != self.kinds[i] {
                self.kinds[i] = k;
                self.slots[i] = Fx::create(k, &mut self.pool, rng).unwrap_or(Fx::None);
            } else if self.stale[i] {
                self.slots[i].clear(rng);
            }
            self.stale[i] = false;
            self.slots[i].run(p, l, r);
        }
    }

    /// Every slot returns to its fresh state when it next runs, at the same point of the random draw order as
    /// a fresh load would create it (memory is kept, so the pool is not consumed again).
    pub fn reset(&mut self) {
        self.stale = [true; SLOTS];
    }

    /// The kind each slot currently holds (`NONE` if it fell back).
    pub fn active(&self, i: usize) -> bool {
        !matches!(self.slots[i], Fx::None)
    }
}
