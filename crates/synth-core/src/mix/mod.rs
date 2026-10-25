//! Shared song/mixer model for all engines. The binary song format is WaveSabre's song blob (as written by
//! `WaveSabreConvert`), so WaveSabre songs load unchanged; device kinds are resolved by an engine-provided
//! [`Kinds`] enum (ids 0–12 WaveSabre, ≥ 32 other engines).

mod player;
mod registry;
mod song;

pub use player::{Player, DEFAULT_SUB};
pub use registry::{devices_need, load_devices, Either, Env, Kinds};
pub use song::{Automation, DeviceRec, Lane, LaneEvents, Points, Receive, Song, Track};

/// Why a song failed to load. Decoding never panics on untrusted bytes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoadError {
    /// Truncated, negative or implausible count, trailing bytes, bad header value.
    Malformed,
    /// A device, lane, track, channel or parameter reference out of range, or a receive cycle feeding the master.
    BadIndex,
    /// A device kind not compiled into this build (or unknown).
    UnsupportedKind(u8),
    /// A device chunk the device cannot read.
    BadChunk,
    OutOfMemory,
}

impl From<crate::pool::OutOfMemory> for LoadError {
    fn from(_: crate::pool::OutOfMemory) -> Self {
        LoadError::OutOfMemory
    }
}
