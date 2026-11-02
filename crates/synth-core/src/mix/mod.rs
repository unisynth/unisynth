//! Shared song/mixer model for all engines. The binary song format is WaveSabre's song blob (as written by
//! `WaveSabreConvert`), so WaveSabre songs load unchanged; device kinds are resolved by an engine-provided
//! [`Kinds`] enum (ids 0–12 WaveSabre, ≥ 32 other engines).

mod mixer;
mod player;
mod registry;
mod song;

pub use mixer::{Mixer, Source, TrackSource};
#[cfg(feature = "threads")]
pub use mixer::{Par, SplitSource};
pub use player::{pcm_i16, BlobSource, Player, TrackSrc, DEFAULT_SUB};
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
    /// The kind exists but a module its chunk uses (oscillator, filter, effect mode, …) is not compiled in. The id
    /// is the engine's own module number. Unlike `UnsupportedKind`, `Either` does not fall back on it.
    UnsupportedModule(u8),
    /// A device chunk the device cannot read.
    BadChunk,
    /// A device chunk in a format version this player does not read (an older layout: migrate it on the host).
    UnknownVersion,
    OutOfMemory,
}

impl From<crate::pool::OutOfMemory> for LoadError {
    fn from(_: crate::pool::OutOfMemory) -> Self {
        LoadError::OutOfMemory
    }
}
