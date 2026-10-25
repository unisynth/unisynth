//! DSP primitives that are the same algorithm in more than one engine. Buffers come from the caller (usually a
//! [`crate::pool::Pool`]); coefficients are set by the engine.

mod allpass;
mod biquad;
mod comb;
mod delay;
mod onepole;

pub use allpass::AllPass;
pub use biquad::{Biquad, Coefs};
pub use comb::Comb;
pub use delay::Ring;
pub use onepole::OnePole;
