//! `tfsynth`: an MIT, `no_std`, f32 synth engine that is sound-compatible with Tunefish 4 (patches and `.tfm`
//! songs). Own implementation from the behavioural spec `research/tunefish4-functional-spec.md`; no upstream code,
//! comments, tables or identifiers. 44 100 Hz, 512-frame blocks, no heap, no `core::fmt`.
#![cfg_attr(not(test), no_std)]

pub mod device;
pub mod env;
pub mod filter;
pub mod fx;
pub mod ifft;
pub mod instrument;
pub mod lfo;
pub mod modmx;
pub mod noise;
pub mod params;
pub mod player;
pub mod rng;
pub mod tables;
pub mod tfm;
pub mod voice;
pub mod wavetable;

#[cfg(test)]
mod tests;
