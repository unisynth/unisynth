//! Distortion, wrap mode (change add-cauldron-xm-import): mini-cauldron's 8-bit wrap-around specials (Stash song 1,
//! slot 9 `x + x>>2`, slots 11 and 15 `x + 0x30`) as a real-time effect: `v = x·drive + offset`, wrapped into −1..1.
//! Parameters after the mode: drive (`p·4`), offset (`2p − 1`).

use super::{Effect, Params};
use synth_core::device::{Ctx, Io};
use synth_core::mix::{Env, LoadError};
use synth_core::pool::Pool;

pub struct Wrap {
    drive: f32,
    offset: f32,
}

/// `((v + 1) mod 2) − 1`.
#[inline]
fn wrap(v: f32) -> f32 {
    let u = v + 1.0;
    u - 2.0 * libm::floorf(u * 0.5) - 1.0
}

impl<'a> Effect<'a> for Wrap {
    const PARAMS: usize = 2;
    fn need(_: &Env, _: &Params) -> usize {
        0
    }
    fn new(_: &Env, _: &mut Pool<'a>) -> Result<Self, LoadError> {
        Ok(Wrap { drive: 1.0, offset: 0.0 })
    }
    fn set(&mut self, i: usize, v: f32) {
        match i {
            0 => self.drive = v * 4.0,
            1 => self.offset = 2.0 * v - 1.0,
            _ => {}
        }
    }
    fn reset(&mut self) {}
    fn run(&mut self, io: &mut Io, n: usize, _: &mut Ctx) {
        let (d, o) = (self.drive, self.offset);
        for c in 0..2 {
            io.ch[c][..n].iter_mut().for_each(|v| *v = wrap(*v * d + o));
        }
    }
    /// No memory; with an offset it plays DC on silence, so its track never sleeps.
    fn quiet_after(&self) -> Option<u32> {
        (self.offset == 0.0).then_some(0)
    }
}
