//! no_std Rust port of the WaveSabre synth devices (upstream github.com/logicomacorp/WaveSabre at 9245fb8,
//! Copyright 2012-2021 WaveSabre Team, MIT — see `LICENSE-WaveSabre`). Devices implement `synth_core::device::Device`
//! and play through `synth_core::mix::Player`; each device is a cargo feature `ws-<name>`.
#![cfg_attr(not(test), no_std)]

#[cfg(feature = "ws-adultery")]
mod adultery;
mod biquad;
mod device;
pub mod helpers;

#[cfg(feature = "ws-cathedral")]
mod cathedral;
#[cfg(feature = "ws-chamber")]
mod chamber;
#[cfg(feature = "ws-crusher")]
mod crusher;
#[cfg(feature = "ws-echo")]
mod echo;
#[cfg(feature = "ws-falcon")]
mod falcon;
#[cfg(any(feature = "ws-specimen", feature = "ws-thunder"))]
pub use gsm;
#[cfg(feature = "ws-leveller")]
mod leveller;
#[cfg(any(feature = "ws-specimen", feature = "ws-adultery"))]
mod sampler;
#[cfg(feature = "ws-scissor")]
mod scissor;
#[cfg(feature = "ws-slaughter")]
mod slaughter;
#[cfg(feature = "ws-smasher")]
mod smasher;
#[cfg(feature = "ws-specimen")]
mod specimen;
mod svf;
#[allow(dead_code)]
mod synth;
#[cfg(feature = "ws-thunder")]
mod thunder;
#[cfg(feature = "ws-twister")]
mod twister;

pub use device::{kind, WsDevice};

#[cfg(test)]
mod tests;
