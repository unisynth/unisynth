//! Shared synth layer for every engine (WaveSabre port, Tunefish-compatible engine): DSP primitives that are the same
//! algorithm in several engines, plus the framework every instrument and effect plugs into. `no_std`, f32, no heap:
//! large state comes from a caller-provided [`pool::Pool`].
#![cfg_attr(not(test), no_std)]

pub mod denormal;
pub mod device;
pub mod dsp;
pub mod math;
pub mod mix;
pub mod pool;
pub mod rng;
pub mod voice;

#[cfg(test)]
mod tests;
