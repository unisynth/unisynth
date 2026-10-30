//! `glorp`: independent `no_std` devices compatible with the proprietary "Glorp" song's own device set, written from
//! observed behaviour only (research/glorp-devices.md); no third-party code, tables or data. Today: the song-used
//! subset of effect ID 38 ([`id38`]), the complete frequency shifter ID 34 ([`id34`]), the second-order subset of the
//! all-pass cascade ID 35 ([`id35`]), the chorus / all-pass-feedback ensemble ID 32 ([`id32`]), the complete look-ahead compressor / limiter ID 7 ([`id7`]) and the complete waveshaper / saturator ID 3 ([`id3`]); the instruments ID 20 ([`id20`], twelve negative-feedback combs excited by shaped noise and a click, velocity sensitive, velocity ceiling 213), ID 21 ([`id21`], additive recursive sine bank, bounded partial count), ID 22 ([`id22`], circle oscillator through an iterated 3D trig map, weight-overflow guarded), ID 25 ([`id25`]) and ID 27 ([`id27`]) on the shared 16-voice scheduler ([`synth`]).
//! The devices keep their standalone, fallible APIs (`Err` on a setting they do not model). [`kinds::GlorpDevice`] wraps
//! them as a `synth_core::mix` `Device` + `Kinds`, and because `Device::set_param` cannot report a refusal the song is
//! judged first ([`preflight`], run by `Kinds::preflight` inside `Player::new`): unsupported ids, chunks, settings, rate
//! and automation lanes that could leave the supported settings are refused before anything is built; sub-block 128 only.
//! [`player::GlorpPlayer`] is the owned entry point with full diagnostics. See `docs/synth/glorp.md`.
#![cfg_attr(not(test), no_std)]

pub mod id14;
pub mod id20;
pub mod id21;
pub mod id22;
pub mod id25;
pub mod id27;
pub mod id28;
pub mod id3;
pub mod id30;
pub mod id32;
pub mod id34;
pub mod id35;
pub mod id38;
pub mod id7;
pub mod kinds;
pub mod player;
pub mod preflight;
pub mod synth;

#[cfg(test)]
mod tests;
#[cfg(test)]
mod testsong;
#[cfg(test)]
mod tests_kinds;
#[cfg(test)]
mod tests_player;
#[cfg(test)]
mod tests14;
#[cfg(test)]
mod tests20;
#[cfg(test)]
mod tests21;
#[cfg(test)]
mod tests22;
#[cfg(test)]
mod tests25;
#[cfg(test)]
mod tests27;
#[cfg(test)]
mod tests28;
#[cfg(test)]
mod tests_mono;
#[cfg(test)]
mod tests_synth;
#[cfg(test)]
mod tests3;
#[cfg(test)]
mod tests30;
#[cfg(test)]
mod tests32;
#[cfg(test)]
mod tests34;
#[cfg(test)]
mod tests35;
#[cfg(test)]
mod tests7;
